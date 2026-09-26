//! Vault KV v2 provider-credential backend.
//!
//! Each resolution re-reads the operator token source and issues exactly one
//! latest-version read-secret request, so rotation and revocation apply to the
//! next attempt. Nothing is cached, and neither the token nor the value ever
//! enters a URL, log, or error. Egress runs under the non-production
//! [`EndpointPolicy`], so legitimate internal Vault addresses stay reachable
//! while metadata and link-local destinations, literal or resolved, never
//! receive the token.

use std::time::Duration;

use reqwest::header::HeaderValue;
use reqwest::{Certificate, Client, Error, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;
use url::Url;
use wyrd_spec::gateway::ExternalSecretReference;
use wyrd_spec::security::SecretRef;

use crate::credential::{CredentialError, read_binding};
use crate::endpoint::EndpointPolicy;

/// Total time one Vault read may take.
const VAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// Time allowed to establish the Vault connection.
const VAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Largest Vault response body read, in bytes; larger bodies are unavailable.
const MAX_VAULT_RESPONSE_BYTES: usize = 256 * 1024;

/// One operator-declared Vault KV v2 backend.
#[derive(Debug, Clone)]
pub struct VaultBackend {
    /// Vault server base address.
    address: Url,
    /// KV v2 mount path, possibly several segments.
    mount: String,
    /// Token source, re-read on every resolution.
    token: SecretRef,
    /// Optional Vault Enterprise namespace.
    namespace: Option<String>,
    /// Egress policy screening the address literal and every resolved answer.
    policy: EndpointPolicy,
    /// Redirect-free, proxy-free, time-bounded client using the configured
    /// trust and the policy's screening resolver.
    client: Client,
}

impl VaultBackend {
    /// Builds a backend for `mount` at `address`.
    ///
    /// `ca_cert` is a PEM bundle that replaces the platform trust roots when
    /// present. The client follows no redirects, consults no proxy, bounds
    /// connect and total time, and resolves hostnames through the
    /// non-production endpoint policy's screening resolver.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when the CA bundle does not parse or the
    /// client cannot be built.
    pub fn new(
        address: Url,
        mount: String,
        token: SecretRef,
        namespace: Option<String>,
        ca_cert: Option<&[u8]>,
    ) -> Result<Self, Error> {
        Self::with_policy(
            address,
            mount,
            token,
            namespace,
            ca_cert,
            EndpointPolicy::new(false),
        )
    }

    /// Builds a backend whose egress `policy` screens the literal address and
    /// every resolved answer; [`VaultBackend::new`] fixes the non-production
    /// policy.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when the CA bundle does not parse or the
    /// client cannot be built.
    fn with_policy(
        address: Url,
        mount: String,
        token: SecretRef,
        namespace: Option<String>,
        ca_cert: Option<&[u8]>,
        policy: EndpointPolicy,
    ) -> Result<Self, Error> {
        // A conflicting provider installed first still serves this client.
        wyrd_tls::install_crypto_provider().ok();
        let mut builder = policy.screened(
            Client::builder()
                .timeout(VAULT_TIMEOUT)
                .connect_timeout(VAULT_CONNECT_TIMEOUT),
        );
        if let Some(pem) = ca_cert {
            builder = builder.tls_certs_only(Certificate::from_pem_bundle(pem)?);
        }
        Ok(Self {
            address,
            mount,
            token,
            namespace,
            policy,
            client: builder.build()?,
        })
    }

    /// Reads the latest version of `reference` and returns the string at
    /// `data.data.<key>`.
    ///
    /// Sends `GET {address}/v1/{mount}/data/{path}` with no `version`
    /// parameter, every path segment percent-encoded, the token in
    /// `X-Vault-Token`, and the namespace in `X-Vault-Namespace` when set.
    ///
    /// # Errors
    ///
    /// - [`CredentialError::Missing`] for HTTP 404, a null or absent secret, a
    ///   missing or non-string key, or a deleted or destroyed latest version.
    /// - [`CredentialError::Unconfigured`] for HTTP 400, 403, or 405, an
    ///   unreadable token source, or an address whose literal IP is blocked,
    ///   refused before the token is read or any request is sent.
    /// - [`CredentialError::Unavailable`] for a timeout, connection failure
    ///   (including a hostname resolving to a blocked address), any other
    ///   status, or an oversized or undecodable body.
    pub async fn fetch(
        &self,
        reference: &ExternalSecretReference,
    ) -> Result<SecretString, CredentialError> {
        if !self.policy.permits_host(&self.address) {
            return Err(CredentialError::Unconfigured);
        }
        let token = read_binding(&self.token)
            .await
            .map_err(|_| CredentialError::Unconfigured)?;
        let mut token = HeaderValue::from_str(token.expose_secret().trim())
            .map_err(|_| CredentialError::Unconfigured)?;
        token.set_sensitive(true);
        let mut url = self.address.clone();
        url.path_segments_mut()
            .map_err(|()| CredentialError::Unconfigured)?
            .pop_if_empty()
            .push("v1")
            .extend(self.mount.split('/'))
            .push("data")
            .extend(reference.path().split('/'));
        let mut request = self.client.get(url).header("X-Vault-Token", token);
        if let Some(namespace) = &self.namespace {
            request = request.header("X-Vault-Namespace", namespace);
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| CredentialError::Unavailable)?;
        match response.status() {
            StatusCode::OK => {}
            StatusCode::NOT_FOUND => return Err(CredentialError::Missing),
            StatusCode::BAD_REQUEST | StatusCode::FORBIDDEN | StatusCode::METHOD_NOT_ALLOWED => {
                return Err(CredentialError::Unconfigured);
            }
            _ => return Err(CredentialError::Unavailable),
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| CredentialError::Unavailable)?
        {
            if body.len() + chunk.len() > MAX_VAULT_RESPONSE_BYTES {
                return Err(CredentialError::Unavailable);
            }
            body.extend_from_slice(&chunk);
        }
        let envelope: Value =
            serde_json::from_slice(&body).map_err(|_| CredentialError::Unavailable)?;
        let metadata = &envelope["data"]["metadata"];
        let deleted = metadata["deletion_time"]
            .as_str()
            .is_some_and(|time| !time.is_empty());
        if deleted || metadata["destroyed"] == Value::Bool(true) {
            return Err(CredentialError::Missing);
        }
        envelope["data"]["data"][reference.key()]
            .as_str()
            .map(SecretString::from)
            .ok_or(CredentialError::Missing)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    /// Canary written as the Vault token; it must reach only the header.
    const TOKEN: &str = "hvs.token-canary";

    /// Backend over `server` with mount `kv/team`, namespace `ns1`, and the
    /// token read from `token`.
    fn backend(address: &str, token: &tempfile::NamedTempFile) -> VaultBackend {
        VaultBackend::new(
            Url::parse(address).expect("address"),
            "kv/team".to_owned(),
            SecretRef::File {
                path: token.path().to_string_lossy().into_owned(),
            },
            Some("ns1".to_owned()),
            None,
        )
        .expect("backend builds")
    }

    /// Restrictive token file holding `value`.
    fn token_file(value: &str) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().expect("token file");
        write!(file, "{value}").expect("token written");
        file
    }

    /// Validated reference fixture.
    fn reference(value: &str) -> ExternalSecretReference {
        ExternalSecretReference::new(value).expect("reference")
    }

    /// Answers the next read of `route` with `status` and `body`.
    async fn answer(server: &MockServer, route: &str, status: u16, body: Value) {
        server.reset().await;
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(status).set_body_json(body))
            .mount(server)
            .await;
    }

    /// A read issues one latest-version KV v2 GET with encoded segments and
    /// the token and namespace headers, and re-reads a rotated token.
    #[tokio::test]
    async fn kv_v2_reads_latest_version_with_current_token() {
        let server = MockServer::start().await;
        let route = "/v1/kv/team/data/openai/a%20b";
        let token = token_file(TOKEN);
        let vault = backend(&server.uri(), &token);
        let target = reference("openai/a b#api_key");
        Mock::given(method("GET"))
            .and(path(route))
            .and(header("x-vault-token", TOKEN))
            .and(header("x-vault-namespace", "ns1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": {"data": {"api_key": "sk-v2"}, "metadata": {"version": 2, "deletion_time": "", "destroyed": false}}
            })))
            .mount(&server)
            .await;
        let value = vault.fetch(&target).await.expect("latest version resolves");
        assert_eq!(value.expose_secret(), "sk-v2");
        let requests = server.received_requests().await.expect("recording");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url.query(), None, "no version parameter");

        std::fs::write(token.path(), "hvs.rotated").expect("token rotates");
        Mock::given(method("GET"))
            .and(header("x-vault-token", "hvs.rotated"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"data": {"data": {"api_key": "sk-v3"}, "metadata": {}}})),
            )
            .with_priority(1)
            .mount(&server)
            .await;
        let value = vault.fetch(&target).await.expect("rotated token reads");
        assert_eq!(value.expose_secret(), "sk-v3");
    }

    /// Every Vault status, body, token, and connection outcome maps onto the
    /// closed resolution result.
    #[tokio::test]
    async fn kv_v2_maps_outcomes() {
        let server = MockServer::start().await;
        let route = "/v1/kv/team/data/openai/a%20b";
        let token = token_file(TOKEN);
        let vault = backend(&server.uri(), &token);
        let target = reference("openai/a b#api_key");
        let oversized = "x".repeat(MAX_VAULT_RESPONSE_BYTES);
        let cases = [
            (404, json!({"errors": []}), CredentialError::Missing),
            (
                200,
                json!({"data": {"data": null, "metadata": {}}}),
                CredentialError::Missing,
            ),
            (
                200,
                json!({"data": {"data": {"other": "x"}}}),
                CredentialError::Missing,
            ),
            (
                200,
                json!({"data": {"data": {"api_key": 7}}}),
                CredentialError::Missing,
            ),
            (
                200,
                json!({"data": {"data": {"api_key": "old"}, "metadata": {"deletion_time": "2026-01-01T00:00:00Z"}}}),
                CredentialError::Missing,
            ),
            (
                200,
                json!({"data": {"data": {"api_key": "old"}, "metadata": {"destroyed": true}}}),
                CredentialError::Missing,
            ),
            (400, json!({}), CredentialError::Unconfigured),
            (
                403,
                json!({"errors": ["permission denied"]}),
                CredentialError::Unconfigured,
            ),
            (405, json!({}), CredentialError::Unconfigured),
            (500, json!({}), CredentialError::Unavailable),
            (307, json!({}), CredentialError::Unavailable),
            (
                200,
                json!({"data": {"data": {"api_key": oversized}}}),
                CredentialError::Unavailable,
            ),
        ];
        for (status, body, expected) in cases {
            answer(&server, route, status, body).await;
            assert_eq!(
                vault.fetch(&target).await.map(|_| ()),
                Err(expected),
                "{status}"
            );
        }
        server.reset().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;
        assert_eq!(
            vault.fetch(&target).await.map(|_| ()),
            Err(CredentialError::Unavailable)
        );

        let unreadable = VaultBackend::new(
            Url::parse(&server.uri()).expect("address"),
            "kv".to_owned(),
            SecretRef::Env {
                name: "WYRD_GATEWAY_VAULT_TEST_UNSET_TOKEN".to_owned(),
            },
            None,
            None,
        )
        .expect("backend builds");
        assert_eq!(
            unreadable.fetch(&target).await.map(|_| ()),
            Err(CredentialError::Unconfigured)
        );
        let address = server.uri();
        drop(server);
        assert_eq!(
            backend(&address, &token).fetch(&target).await.map(|_| ()),
            Err(CredentialError::Unavailable)
        );
    }

    /// Metadata and link-local literal addresses are refused before any
    /// request, a hostname is screened at resolution with the connection
    /// pinned to its screened answers (a production-profile client refuses the
    /// loopback fixture without it receiving a request), and the default
    /// client still reaches the internal fixture.
    ///
    /// # Panics
    ///
    /// Panics when the token file, fixture address, or backend cannot be
    /// built, when a screened address is not refused, when the fixture records
    /// a request, or when the internal fixture does not resolve.
    #[tokio::test]
    async fn vault_egress_is_screened_and_pinned() {
        let token = token_file(TOKEN);
        let target = reference("openai/a b#api_key");
        for literal in [
            "http://169.254.169.254",
            "http://100.100.100.200:8200",
            "http://[fe80::1]:8200",
            "http://[::ffff:169.254.169.254]",
        ] {
            assert_eq!(
                backend(literal, &token).fetch(&target).await.map(|_| ()),
                Err(CredentialError::Unconfigured),
                "{literal}"
            );
        }
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"data": {"data": {"api_key": "sk-internal"}}})),
            )
            .mount(&server)
            .await;
        let address = format!("http://localhost:{}", server.address().port());
        let screened = VaultBackend::with_policy(
            Url::parse(&address).expect("address"),
            "kv/team".to_owned(),
            SecretRef::File {
                path: token.path().to_string_lossy().into_owned(),
            },
            None,
            None,
            EndpointPolicy::new(true),
        )
        .expect("backend builds");
        assert_eq!(
            screened.fetch(&target).await.map(|_| ()),
            Err(CredentialError::Unavailable)
        );
        assert!(
            server
                .received_requests()
                .await
                .expect("recording")
                .is_empty()
        );
        let internal = backend(&address, &token)
            .fetch(&target)
            .await
            .expect("internal Vault resolves");
        assert_eq!(internal.expose_secret(), "sk-internal");
    }
}
