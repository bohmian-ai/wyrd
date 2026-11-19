//! Local Vault Transit protocol fixture for journeys that vary the `[kms]`
//! wrapping backend.
//!
//! A journey boots a server on [`TransitFixture::config`] instead of the
//! default generated file key, then counts the wrap and unwrap requests the
//! fixture served to prove its stored secrets were sealed and opened through
//! the remote backend rather than another local path.

use std::time::Duration;

use base64::Engine as _;
use serde_json::{Value, json};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};
use wyrd_kms::{KmsBackendKind, KmsConfig};

/// Transit token every [`transit_config`] carries; the fixture accepts any.
const TRANSIT_TOKEN: &str = "fixture-vault-token";

/// A running local Transit endpoint.
///
/// An encrypt answers `vault:v1:` ciphertext carrying `{context, plaintext}`,
/// and a decrypt under another context answers 400, the way Transit
/// authenticates associated data. Every key name is accepted.
pub struct TransitFixture {
    /// Mock HTTP server answering `/v1/<mount>/encrypt|decrypt/<key>`.
    server: MockServer,
}

impl TransitFixture {
    /// Start a fixture on a loopback port.
    pub async fn start() -> Self {
        Self::start_delayed(Duration::ZERO).await
    }

    /// Start a fixture that answers every request after `delay`, so a
    /// journey can commit a competing write while an encryption is still in
    /// flight. A request is counted by [`Self::requests`] on arrival.
    pub async fn start_delayed(delay: Duration) -> Self {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(Transit { delay })
            .mount(&server)
            .await;
        Self { server }
    }

    /// Base URL of the fixture.
    #[must_use]
    pub fn uri(&self) -> String {
        self.server.uri()
    }

    /// A `vault_transit` `[kms]` pointed at this fixture, sealing under
    /// `active` and opening `retained` too.
    #[must_use]
    pub fn config(&self, active: &str, retained: &[&str]) -> KmsConfig {
        transit_config(&self.uri(), active, retained)
    }

    /// Number of `operation` (`encrypt` or `decrypt`) requests served so far.
    ///
    /// # Panics
    /// Panics when the mock server does not record requests.
    pub async fn requests(&self, operation: &str) -> usize {
        let segment = format!("/{operation}/");
        self.server
            .received_requests()
            .await
            .expect("transit fixture records requests")
            .iter()
            .filter(|request| request.url.path().contains(&segment))
            .count()
    }
}

/// A `vault_transit` `[kms]` at `endpoint` sealing under `active` and
/// opening `retained` too, with an inline token.
///
/// Use it directly for an endpoint that is not a [`TransitFixture`], such as
/// a stalled or closed port.
#[must_use]
pub fn transit_config(endpoint: &str, active: &str, retained: &[&str]) -> KmsConfig {
    KmsConfig {
        backend: Some(KmsBackendKind::VaultTransit),
        active_key: Some(active.to_owned()),
        retained_keys: retained.iter().map(|key| (*key).to_owned()).collect(),
        endpoint: Some(endpoint.to_owned()),
        vault_token: Some(secrecy::SecretString::from(TRANSIT_TOKEN)),
        ..KmsConfig::default()
    }
}

/// The Transit protocol responder behind [`TransitFixture`].
struct Transit {
    /// How long every answer is held back.
    delay: Duration,
}

impl Respond for Transit {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        Self::answer(request).set_delay(self.delay)
    }
}

impl Transit {
    /// The undelayed answer to `request`.
    fn answer(request: &Request) -> ResponseTemplate {
        let engine = base64::engine::general_purpose::STANDARD;
        let Ok(body) = serde_json::from_slice::<Value>(&request.body) else {
            return ResponseTemplate::new(400).set_body_json(json!({"errors": ["invalid json"]}));
        };
        if request.url.path().contains("/encrypt/") {
            let blob = json!({"context": body["associated_data"], "plaintext": body["plaintext"]});
            let ciphertext = format!("vault:v1:{}", engine.encode(blob.to_string()));
            return ResponseTemplate::new(200)
                .set_body_json(json!({"data": {"ciphertext": ciphertext}}));
        }
        let blob: Value = body["ciphertext"]
            .as_str()
            .and_then(|text| text.strip_prefix("vault:v1:"))
            .and_then(|encoded| engine.decode(encoded).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        if blob["context"] != body["associated_data"] {
            return ResponseTemplate::new(400)
                .set_body_json(json!({"errors": ["cipher: message authentication failed"]}));
        }
        ResponseTemplate::new(200).set_body_json(json!({"data": {"plaintext": blob["plaintext"]}}))
    }
}
