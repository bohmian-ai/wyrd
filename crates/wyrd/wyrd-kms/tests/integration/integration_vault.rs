//! `WyrdKms` over a real `vault server -dev` Transit engine, run by
//! `test:kms:vault`.
//!
//! The adapter's request shapes are pinned by unit tests against a local
//! protocol fixture; what only a real server proves is that Transit accepts
//! the facade's `associated_data` on an `aes256-gcm96` key, refuses a
//! different context, keeps old envelopes readable across native key
//! rotation, and refuses a token it does not accept. The dev server holds its
//! state in memory and dies with the test.

use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use reqwest::Client;
use secrecy::SecretString;
use serde_json::json;
use wyrd_kms::{
    KmsBackendKind, KmsConfig, KmsError, ProviderFailure, SecretContext, SecretPurpose,
    SecretScope, WyrdKms,
};
use wyrd_spec::DataTenantId;

/// Root token the dev server is launched with; a fixed test constant.
const ROOT_TOKEN: &str = "wyrd-test-kms-root";

/// Transit key the facade is configured with.
const KEY: &str = "wyrd-secrets";

/// Bound the dev server must become healthy within.
const READY_BOUND: Duration = Duration::from_secs(30);

/// A `vault server -dev` child, killed on drop.
struct DevVault {
    /// The running dev server.
    child: Child,
    /// Root of the HTTP API.
    address: String,
    /// Administration client.
    http: Client,
}

impl Drop for DevVault {
    /// Kills the dev server, ignoring a child that already exited.
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl DevVault {
    /// Launch on a free loopback port and wait until healthy.
    ///
    /// # Panics
    /// Panics when no port is free, `vault` is not on `PATH`, or the server
    /// is not healthy within [`READY_BOUND`].
    async fn start() -> Self {
        let port = TcpListener::bind("127.0.0.1:0")
            .and_then(|listener| listener.local_addr())
            .expect("a free loopback port")
            .port();
        let listen = format!("127.0.0.1:{port}");
        let child = Command::new("vault")
            .args([
                "server",
                "-dev",
                "-dev-no-store-token",
                &format!("-dev-root-token-id={ROOT_TOKEN}"),
                &format!("-dev-listen-address={listen}"),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("`vault` is on PATH; test:kms:vault installs it through mise");
        let vault = Self {
            child,
            address: format!("http://{listen}"),
            http: Client::new(),
        };
        let deadline = Instant::now() + READY_BOUND;
        loop {
            let health = vault
                .http
                .get(format!("{}/v1/sys/health", vault.address))
                .send()
                .await;
            if health.is_ok_and(|response| response.status().is_success()) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "dev Vault did not become healthy"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        vault
    }

    /// POST `body` to `route` as root and require success.
    ///
    /// # Panics
    /// Panics when Vault refuses the call.
    async fn admin(&self, route: &str, body: serde_json::Value) {
        let response = self
            .http
            .post(format!("{}/v1/{route}", self.address))
            .header("X-Vault-Token", ROOT_TOKEN)
            .json(&body)
            .send()
            .await
            .expect("Vault answers");
        assert!(
            response.status().is_success(),
            "{route}: {}",
            response.status()
        );
    }

    /// A facade over this server's Transit mount with `token`.
    fn kms(&self, token: &str) -> WyrdKms {
        WyrdKms::new(
            &KmsConfig {
                backend: Some(KmsBackendKind::VaultTransit),
                active_key: Some(KEY.to_owned()),
                endpoint: Some(self.address.clone()),
                vault_token: Some(SecretString::from(token)),
                ..KmsConfig::default()
            },
            false,
        )
        .expect("facade builds")
    }
}

/// Seal and open through real Transit, refuse a substituted context, keep an
/// old envelope readable after native rotation, and refuse a bad token.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires a `vault` binary; run by test:kms:vault"]
async fn transit_wraps_rotates_and_refuses_through_the_facade() {
    let vault = DevVault::start().await;
    vault
        .admin("sys/mounts/transit", json!({"type": "transit"}))
        .await;
    vault
        .admin(
            &format!("transit/keys/{KEY}"),
            json!({"type": "aes256-gcm96"}),
        )
        .await;
    let kms = vault.kms(ROOT_TOKEN);
    let ctx = SecretContext {
        scope: SecretScope::Tenant(DataTenantId::new_v7()),
        purpose: SecretPurpose::OperatorConnection,
        owner: "conn-1",
        authority: &["slack", "ops"],
        version: Some(1),
    };
    let sealed = kms.seal(&ctx, b"xoxb-token").await.expect("seals");
    assert_eq!(
        kms.open(&ctx, &sealed).await.expect("opens").as_slice(),
        b"xoxb-token"
    );
    let refused = Err(KmsError::Unavailable {
        backend: KmsBackendKind::VaultTransit,
        failure: ProviderFailure::Refused,
    });
    let other = SecretContext {
        owner: "conn-2",
        ..ctx
    };
    assert_eq!(kms.open(&other, &sealed).await.map(|_| ()), refused);

    vault
        .admin(&format!("transit/keys/{KEY}/rotate"), json!({}))
        .await;
    assert_eq!(
        kms.open(&ctx, &sealed)
            .await
            .expect("old version opens")
            .as_slice(),
        b"xoxb-token"
    );
    let rewrapped = kms.rewrap(&ctx, &sealed).await.expect("rewraps onto v2");
    assert_ne!(rewrapped, sealed);
    assert_eq!(
        kms.open(&ctx, &rewrapped).await.expect("opens").as_slice(),
        b"xoxb-token"
    );

    assert_eq!(
        vault
            .kms("hvs.not-a-token")
            .open(&ctx, &sealed)
            .await
            .map(|_| ()),
        refused
    );
}
