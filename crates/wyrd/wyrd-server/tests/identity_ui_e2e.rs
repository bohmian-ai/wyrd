//! Production UI identity journey host.
//!
//! Starts one real Wyrd server on a bound socket with the deployment BFF
//! service key and public origin, seeds an SSO tenant (Active Keycloak
//! connection), an OIDC-off tenant (API keys only), three switch tenants (two
//! on the same Keycloak issuer, one on Dex, a different provider), and a
//! provider-replacement tenant (Active first realm, replaced by the second
//! Keycloak realm through the settings page), starts two production
//! BFF processes (`node build`) against that same server and Postgres with the
//! same public origin — the first over loopback HTTP, the second only through
//! a Node TLS terminator whose certificate the repository's test CA issues and
//! the replica trusts — and runs the Vitest HTTP journey
//! `src/lib/server/auth/production-auth.integration.test.ts` against them. The
//! journey drives both replicas over HTTP like a browser behind a load
//! balancer; no Vitest mock stands in for the BFF or the server.
//!
//! Ignored so the family lanes, which start no identity provider or BFF, skip
//! it visibly; `mise run test:identity:journey` builds the UI and runs it.
//!
//! Every connection is activated as a deployment would: staged, tested
//! through one real provider sign-in returned to the server's callback, and
//! activated with the tenant's recovery key.

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use secrecy::ExposeSecret as _;
use serde_json::{Value, json};
use wyrd_testing::bifrost::peer_ca::BifrostPeerCa;
use wyrd_testing::{WyrdTestServer, WyrdTestServerBuilder, provider_sign_in};

/// Keycloak realm issuer the lane's compose service serves.
fn keycloak_issuer() -> String {
    std::env::var("WYRD_KEYCLOAK_ISSUER")
        .unwrap_or_else(|_| "http://localhost:18080/realms/wyrd-test".to_owned())
}

/// Dex issuer the lane's compose service serves: the journey's second,
/// different provider.
fn dex_issuer() -> String {
    std::env::var("WYRD_DEX_ISSUER").unwrap_or_else(|_| "http://localhost:5556".to_owned())
}

/// A provider account a connection test signs in as: `(username, password)`.
type ProviderUser = (&'static str, &'static str);

/// Keycloak's `alice`, present in both realms.
const KEYCLOAK_ALICE: ProviderUser = ("alice", "alice-password");

/// Dex's static password-database user.
const DEX_ALICE: ProviderUser = ("alice@wyrd.test", "wyrd-test");

/// The fixture SSO tenant's route key.
const SSO_TENANT: &str = "test-tenant-1";
/// The seeded OIDC-off tenant's route key.
const API_KEY_TENANT: &str = "ui-oidc-off";
/// Switch tenant whose Active connection is Keycloak.
const SWITCH_KEYCLOAK_TENANT: &str = "ui-switch-keycloak";
/// Switch tenant whose Active connection is Dex.
const SWITCH_SECOND_TENANT: &str = "ui-switch-second";
/// Switch tenant on the same Keycloak issuer and client as
/// [`SWITCH_KEYCLOAK_TENANT`], for same-issuer cross-tenant refusal.
const SWITCH_PEER_TENANT: &str = "ui-switch-peer";
/// Tenant whose first-realm connection is replaced by the second realm
/// through settings.
const REPLACEMENT_TENANT: &str = "ui-replace";

/// A journey child process (a BFF replica or the TLS terminator) killed when
/// the journey ends, however it ends.
struct Bff {
    /// The running `node` process.
    child: Child,
    /// The origin the process listens on.
    url: String,
}

impl Drop for Bff {
    /// Kill and reap the process so no child outlives the journey.
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// An unused loopback port.
///
/// # Panics
/// Panics when no port can be bound.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("loopback port binds")
        .local_addr()
        .expect("bound port has an address")
        .port()
}

/// The UI package directory.
fn ui_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("wyrd-ui")
}

/// Start one production BFF replica against `server_url` and wait until it
/// answers. `extra_ca` is the PEM file Node adds to its trust store, so a
/// replica given the TLS terminator's `https:` origin verifies it through the
/// ordinary native `fetch` path.
///
/// # Panics
/// Panics when the build output is missing or the process never answers.
async fn start_bff(
    origin: &str,
    server_url: &str,
    service_key: &str,
    extra_ca: Option<&std::path::Path>,
) -> Bff {
    let ui = ui_dir();
    assert!(
        ui.join("build/index.js").is_file(),
        "build the UI first: pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build"
    );
    let port = free_port();
    let mut node = Command::new("node");
    if let Some(ca) = extra_ca {
        node.env("NODE_EXTRA_CA_CERTS", ca);
    }
    let child = node
        .arg("build")
        .current_dir(&ui)
        .env("HOST", "127.0.0.1")
        .env("PORT", port.to_string())
        .env("ORIGIN", origin)
        .env("WYRD_SERVER_URL", server_url)
        .env("WYRD_BFF_SERVICE_KEY", service_key)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("node starts the BFF");
    let bff = Bff {
        child,
        url: format!("http://127.0.0.1:{port}"),
    };
    let client = reqwest::Client::new();
    for _ in 0..100 {
        if client.get(format!("{}/", bff.url)).send().await.is_ok() {
            return bff;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("BFF on port {port} never answered");
}

/// Node TLS terminator: accepts TLS with `TLS_KEY`/`TLS_CERT` on `TLS_PORT`
/// (dual-stack, so `localhost` resolves either way) and pipes each connection
/// to the loopback server on `UPSTREAM_PORT`.
const TLS_TERMINATOR_JS: &str = "\
const tls = require('node:tls'), net = require('node:net'), fs = require('node:fs');\
const env = process.env;\
tls.createServer({ key: fs.readFileSync(env.TLS_KEY), cert: fs.readFileSync(env.TLS_CERT) }, (client) => {\
  const upstream = net.connect(Number(env.UPSTREAM_PORT), '127.0.0.1');\
  client.on('error', () => upstream.destroy());\
  upstream.on('error', () => client.destroy());\
  client.pipe(upstream).pipe(client);\
}).listen(Number(env.TLS_PORT));";

/// Front the bound server with a TLS terminator whose `localhost` leaf is
/// issued by the repository's test certificate authority, and return it with
/// the CA file a BFF must trust.
///
/// The private BFF channel must be `https:` off loopback; this gives one
/// replica a real trusted TLS hop to the same server.
///
/// # Panics
/// Panics when the material cannot be minted or written, or the terminator
/// never accepts connections.
async fn start_tls_terminator(server_url: &str, material: &std::path::Path) -> (Bff, PathBuf) {
    let upstream = reqwest::Url::parse(server_url)
        .expect("server URL parses")
        .port()
        .expect("bound server URL names its port");
    let tls = BifrostPeerCa::generate("localhost")
        .expect("test CA generates")
        .materialize(material, "ui-server")
        .expect("TLS material writes");
    let port = free_port();
    let child = Command::new("node")
        .args(["-e", TLS_TERMINATOR_JS])
        .env("TLS_KEY", &tls.private_key_path)
        .env("TLS_CERT", &tls.certificate_path)
        .env("TLS_PORT", port.to_string())
        .env("UPSTREAM_PORT", upstream.to_string())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("node starts the TLS terminator");
    let terminator = Bff {
        child,
        url: format!("https://localhost:{port}"),
    };
    for _ in 0..100 {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return (terminator, tls.ca_path);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("TLS terminator on port {port} never accepted");
}

/// Call one authenticated JSON route on the bound server.
///
/// # Panics
/// Panics when the request fails or does not return `200`.
async fn call(
    server: &str,
    token: &str,
    method: reqwest::Method,
    path: &str,
    body: Value,
) -> Value {
    let response = reqwest::Client::new()
        .request(method, format!("{server}{path}"))
        .header("x-wyrd-access-token", format!("Bearer {token}"))
        .json(&body)
        .send()
        .await
        .expect("server answers");
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    assert!(status.is_success(), "{path} returned {status}: {body}");
    body
}

/// Exchange an API key for an access token over HTTP.
///
/// # Panics
/// Panics when the exchange fails.
async fn access_token(server: &str, api_key: &str) -> String {
    let body: Value = reqwest::Client::new()
        .post(format!("{server}/auth/token"))
        .form(&[
            (
                "grant_type",
                "urn:ietf:params:oauth:grant-type:token-exchange",
            ),
            ("subject_token", api_key),
            ("subject_token_type", "urn:wyrd:oauth:token-type:api_key"),
        ])
        .send()
        .await
        .expect("token endpoint answers")
        .json()
        .await
        .expect("token body is JSON");
    body["access_token"]
        .as_str()
        .unwrap_or_else(|| panic!("api key exchanges: {body}"))
        .to_owned()
}

/// The public Keycloak `wyrd-human` connection, granting `admin` to
/// `wyrd-admins` and `reader` to `wyrd-viewers`.
fn keycloak_connection() -> Value {
    json!({
        "issuer": keycloak_issuer(),
        "client_id": "wyrd-human",
        "client_auth": "Public",
        "claim_mapping": { "subject": "sub", "email": "email", "groups": "groups" },
        "group_role_map": { "wyrd-admins": ["admin"], "wyrd-viewers": ["reader"] },
    })
}

/// The public Dex `wyrd-human` connection. Dex asserts no groups, so its
/// users hold no roles.
fn dex_connection() -> Value {
    json!({
        "issuer": dex_issuer(),
        "client_id": "wyrd-human",
        "client_auth": "Public",
        "claim_mapping": { "subject": "sub", "email": "email" },
    })
}

/// Stage, test, and activate `connection` as the tenant's Active human
/// connection, using `admin_key` as both caller and recovery key.
///
/// The test returns a provider authorization URL; `user` signs in there, and
/// the provider's return is sent to the server's `/auth/callback` as the
/// gateway routes it, which marks the candidate tested before activation.
///
/// # Panics
/// Panics when any step fails.
async fn activate_connection(server: &str, admin_key: &str, connection: Value, user: ProviderUser) {
    let token = access_token(server, admin_key).await;
    let candidate = call(
        server,
        &token,
        reqwest::Method::PUT,
        "/v1/identity/oidc/candidate",
        connection,
    )
    .await;
    let revision = candidate["revision"].clone();
    let begun = call(
        server,
        &token,
        reqwest::Method::POST,
        "/v1/identity/oidc/candidate/test",
        json!({ "expected_revision": revision }),
    )
    .await;
    let authorization_url: reqwest::Url = begun["authorization_url"]
        .as_str()
        .expect("test returns an authorization URL")
        .parse()
        .expect("authorization URL parses");
    let (_, callback) = authorization_url
        .query_pairs()
        .find(|(name, _)| name == "redirect_uri")
        .expect("the sign-in names the deployment callback");
    let returned = provider_sign_in(&authorization_url, user.0, user.1, &callback).await;
    let reply = reqwest::Client::new()
        .get(format!(
            "{server}/auth/callback?{}",
            returned.query().unwrap_or_default()
        ))
        .send()
        .await
        .expect("callback answers");
    let status = reply.status();
    let page = reply.text().await.unwrap_or_default();
    assert!(
        status.is_success() && page.contains("Connection test complete"),
        "the test sign-in completes: {status} {page}"
    );
    call(
        server,
        &token,
        reqwest::Method::POST,
        "/v1/identity/oidc/candidate/activate",
        json!({ "expected_revision": revision, "recovery_api_key": admin_key }),
    )
    .await;
}

/// Seed tenant `slug` with a headless `admin` owner and activate `connection`
/// for it, testing it as `user`; returns the owner's API key, the tenant's
/// recovery credential.
///
/// # Panics
/// Panics when seeding, bootstrapping, or activation fails.
async fn sso_tenant(
    srv: &WyrdTestServer,
    server: &str,
    slug: &str,
    connection: Value,
    user: ProviderUser,
) -> String {
    let tenant = srv.seed_tenant(slug).await.expect("tenant seeds");
    let owner = srv
        .bootstrap_service_in_tenant(tenant, &format!("{slug}-owner"), &["admin"])
        .await
        .expect("tenant owner bootstraps");
    let key = owner
        .api_key()
        .expect("owner key")
        .expose_secret()
        .to_owned();
    activate_connection(server, &key, connection, user).await;
    key
}

/// The production UI journey over two BFF replicas and one real server.
///
/// Access tokens live 30 seconds, inside the one-minute renewal margin, so
/// every session read renews through the ordinary issuance path: both
/// replicas contend for the same row lock. A deactivated connection stops
/// renewal at once, but each session keeps its issued token until that
/// token's stored expiry and ends at its first use afterwards.
///
/// # Panics
/// Panics when any setup step fails or the Vitest journey does not pass.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Keycloak and a built UI; run via `mise run test:identity:journey`"]
async fn production_ui_bff_journey() {
    let origin = format!("http://localhost:{}", free_port());
    let service_key = uuid::Uuid::new_v4().simple().to_string();
    let srv = WyrdTestServerBuilder::default()
        .with_public_origin(origin.parse().expect("origin parses"))
        .with_ui_client_secret(&service_key)
        .with_access_ttl(chrono::Duration::seconds(30))
        .start_bound()
        .await
        .expect("server starts");
    let server = srv.base_url().expect("bound server has a URL").to_owned();

    let sso_admin = srv
        .bootstrap_service_in_tenant(srv.data_tenant_id(), "ui-sso-admin", &["admin"])
        .await
        .expect("sso admin bootstraps");
    let sso_admin_key = sso_admin
        .api_key()
        .expect("admin key")
        .expose_secret()
        .to_owned();
    activate_connection(
        &server,
        &sso_admin_key,
        keycloak_connection(),
        KEYCLOAK_ALICE,
    )
    .await;

    let off = srv.seed_tenant(API_KEY_TENANT).await.expect("tenant seeds");
    let off_admin = srv
        .bootstrap_service_in_tenant(off, "ui-off-admin", &["admin"])
        .await
        .expect("oidc-off admin bootstraps");
    let off_reader = srv
        .bootstrap_service_in_tenant(off, "ui-off-reader", &["reader"])
        .await
        .expect("oidc-off reader bootstraps");

    sso_tenant(
        &srv,
        &server,
        SWITCH_KEYCLOAK_TENANT,
        keycloak_connection(),
        KEYCLOAK_ALICE,
    )
    .await;
    sso_tenant(
        &srv,
        &server,
        SWITCH_SECOND_TENANT,
        dex_connection(),
        DEX_ALICE,
    )
    .await;
    sso_tenant(
        &srv,
        &server,
        SWITCH_PEER_TENANT,
        keycloak_connection(),
        KEYCLOAK_ALICE,
    )
    .await;
    let replacement_owner_key = sso_tenant(
        &srv,
        &server,
        REPLACEMENT_TENANT,
        keycloak_connection(),
        KEYCLOAK_ALICE,
    )
    .await;

    // Replica 0 keeps the loopback HTTP topology; replica 1 reaches the same
    // server only through a trusted TLS origin, so every journey step it
    // serves crosses a real TLS hop.
    let material = std::env::temp_dir().join(format!("wyrd-ui-tls-{}", uuid::Uuid::new_v4()));
    let (terminator, ca) = start_tls_terminator(&server, &material).await;
    let first = start_bff(&origin, &server, &service_key, None).await;
    let second = start_bff(&origin, &terminator.url, &service_key, Some(&ca)).await;
    let fixture = json!({
        "origin": origin,
        "server": server,
        "bffs": [first.url, second.url],
        "serviceKey": service_key,
        "ssoTenant": SSO_TENANT,
        "apiKeyTenant": API_KEY_TENANT,
        "ssoAdminKey": sso_admin_key,
        "offAdminKey": off_admin.api_key().expect("admin key").expose_secret(),
        "offReaderKey": off_reader.api_key().expect("reader key").expose_secret(),
        "switchTenants": {
            "keycloak": SWITCH_KEYCLOAK_TENANT,
            "second": SWITCH_SECOND_TENANT,
            "peer": SWITCH_PEER_TENANT,
        },
        "replacementTenant": REPLACEMENT_TENANT,
        "replacementOwnerKey": replacement_owner_key,
        "users": {
            "admin": { "username": "alice", "password": "alice-password" },
            "reader": { "username": "bob", "password": "wyrd-test" },
            "dex": { "login": DEX_ALICE.0, "password": DEX_ALICE.1 },
        },
    });

    let mut vitest = Command::new("pnpm");
    vitest
        .current_dir(ui_dir())
        .args([
            "exec",
            "vitest",
            "run",
            "src/lib/server/auth/production-auth.integration.test.ts",
        ])
        .env("WYRD_UI_INTEGRATION", "1")
        .env("WYRD_UI_JOURNEY", fixture.to_string());
    if let Ok(filter) = std::env::var("WYRD_UI_FILTER") {
        vitest.args(["-t", &filter]);
    }
    let status = tokio::task::spawn_blocking(move || vitest.status())
        .await
        .expect("vitest task joins")
        .expect("vitest runs");
    drop((first, second, terminator));
    let _ = std::fs::remove_dir_all(&material);
    srv.shutdown().await.expect("server shuts down");
    assert!(
        status.success(),
        "the production UI journey failed: {status}"
    );
}
