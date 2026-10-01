//! Production UI identity journey host.
//!
//! Starts one real Wyrd server on a bound socket with the deployment BFF
//! service key and public origin, seeds an SSO tenant (Active Keycloak
//! connection) and an OIDC-off tenant (API keys only), starts two production
//! BFF processes (`node build`) against that same server and Postgres with the
//! same public origin, and runs the Vitest HTTP journey
//! `src/lib/server/auth/production-auth.integration.test.ts` against them. The
//! journey drives both replicas over HTTP like a browser behind a load
//! balancer; no Vitest mock stands in for the BFF or the server.
//!
//! Ignored so the family lanes, which start no identity provider or BFF, skip
//! it visibly; `mise run test:identity:journey` builds the UI and runs it.

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use secrecy::ExposeSecret as _;
use serde_json::{Value, json};
use wyrd_testing::WyrdTestServerBuilder;

/// Keycloak realm issuer the lane's compose service serves.
fn keycloak_issuer() -> String {
    std::env::var("WYRD_KEYCLOAK_ISSUER")
        .unwrap_or_else(|_| "http://localhost:18080/realms/wyrd-test".to_owned())
}

/// The fixture SSO tenant's route key.
const SSO_TENANT: &str = "test-tenant-1";
/// The seeded OIDC-off tenant's route key.
const API_KEY_TENANT: &str = "ui-oidc-off";

/// A BFF process killed when the journey ends, however it ends.
struct Bff {
    /// The running `node build` process.
    child: Child,
    /// `http://127.0.0.1:{port}` the process listens on.
    url: String,
}

impl Drop for Bff {
    /// Kill and reap the process so no BFF outlives the journey.
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

/// Start one production BFF replica and wait until it answers.
///
/// # Panics
/// Panics when the build output is missing or the process never answers.
async fn start_bff(origin: &str, server_url: &str, service_key: &str) -> Bff {
    let ui = ui_dir();
    assert!(
        ui.join("build/index.js").is_file(),
        "build the UI first: pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build"
    );
    let port = free_port();
    let child = Command::new("node")
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
        .bearer_auth(token)
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
        .json(&json!({ "grant_type": "wyrd_api_key", "api_key": api_key }))
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

/// Stage, test, and activate the SSO tenant's public Keycloak connection,
/// granting `admin` to `wyrd-admins` and `reader` to `wyrd-viewers`.
///
/// # Panics
/// Panics when any step fails.
async fn activate_sso(server: &str, admin_key: &str) {
    let token = access_token(server, admin_key).await;
    let candidate = call(
        server,
        &token,
        reqwest::Method::PUT,
        "/v1/identity/oidc/candidate",
        json!({
            "issuer": keycloak_issuer(),
            "client_id": "wyrd-human",
            "client_auth": "Public",
            "claim_mapping": { "subject": "sub", "email": "email", "groups": "groups" },
            "group_role_map": { "wyrd-admins": ["admin"], "wyrd-viewers": ["reader"] },
        }),
    )
    .await;
    let revision = candidate["revision"].clone();
    call(
        server,
        &token,
        reqwest::Method::POST,
        "/v1/identity/oidc/candidate/test",
        json!({ "expected_revision": revision }),
    )
    .await;
    call(
        server,
        &token,
        reqwest::Method::POST,
        "/v1/identity/oidc/candidate/activate",
        json!({ "expected_revision": revision, "recovery_api_key": admin_key }),
    )
    .await;
}

/// The production UI journey over two BFF replicas and one real server.
///
/// Access tokens live 30 seconds, inside the one-minute renewal margin, so
/// every session read renews through the ordinary issuance path: both
/// replicas contend for the same row lock, and a deactivated connection or a
/// revoked key ends the session at its next use.
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
        .with_bff_service_key(&service_key)
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
    activate_sso(&server, &sso_admin_key).await;

    let off = srv.seed_tenant(API_KEY_TENANT).await.expect("tenant seeds");
    let off_admin = srv
        .bootstrap_service_in_tenant(off, "ui-off-admin", &["admin"])
        .await
        .expect("oidc-off admin bootstraps");
    let off_reader = srv
        .bootstrap_service_in_tenant(off, "ui-off-reader", &["reader"])
        .await
        .expect("oidc-off reader bootstraps");

    let first = start_bff(&origin, &server, &service_key).await;
    let second = start_bff(&origin, &server, &service_key).await;
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
        "users": {
            "admin": { "username": "alice", "password": "alice-password" },
            "reader": { "username": "bob", "password": "wyrd-test" },
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
    drop((first, second));
    srv.shutdown().await.expect("server shuts down");
    assert!(
        status.success(),
        "the production UI journey failed: {status}"
    );
}
