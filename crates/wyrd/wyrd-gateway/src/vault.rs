//! Vault KV v2 provider-credential backend.
//!
//! Each resolution re-reads the operator token source and reads the latest
//! version of the referenced secret through the shared [`VaultKv2`] reader, so
//! rotation and revocation apply to the next attempt. Nothing is cached, and
//! neither the token nor the value ever enters a URL, log, or error. The
//! reader owns the network read: bounded time and body, the token and
//! namespace headers, and screened, pinned egress that keeps internal Vault
//! addresses reachable while metadata and link-local destinations never
//! receive the token. This module owns only the provider-credential meaning:
//! the token source and the mapping onto [`CredentialError`].

use secrecy::SecretString;
use url::Url;
use wyrd_spec::gateway::ExternalSecretReference;
use wyrd_spec::security::SecretRef;
use wyrd_vault::{VaultError, VaultKv2};

use crate::credential::{CredentialError, read_binding};

/// One operator-declared Vault KV v2 provider-credential backend.
#[derive(Debug, Clone)]
pub struct VaultBackend {
    /// Shared bounded, screened, and pinned KV v2 reader.
    reader: VaultKv2,
    /// Token source, re-read on every resolution.
    token: SecretRef,
}

impl VaultBackend {
    /// Builds a backend for `mount` at `address`.
    ///
    /// `ca_cert` is a PEM bundle that replaces the platform trust roots when
    /// present; the shared reader fixes every other transport bound.
    ///
    /// # Errors
    ///
    /// Returns [`reqwest::Error`] when the CA bundle does not parse or the
    /// client cannot be built.
    pub fn new(
        address: Url,
        mount: String,
        token: SecretRef,
        namespace: Option<String>,
        ca_cert: Option<&[u8]>,
    ) -> Result<Self, reqwest::Error> {
        Ok(Self {
            reader: VaultKv2::new(address, &mount, namespace, ca_cert)?,
            token,
        })
    }

    /// Reads the latest version of `reference` and returns the string at
    /// `data.data.<key>`.
    ///
    /// # Errors
    ///
    /// - [`CredentialError::Missing`] for HTTP 404, a null or absent secret, a
    ///   missing or non-string key, or a deleted or destroyed latest version.
    /// - [`CredentialError::Unconfigured`] for an unreadable token source, a
    ///   refused read (HTTP 400, 403, or 405), or an address whose literal IP
    ///   is blocked, refused before any request is sent.
    /// - [`CredentialError::Unavailable`] for a timeout, connection failure
    ///   (including a hostname resolving to a blocked address), any other
    ///   status, or an oversized or undecodable body.
    pub async fn fetch(
        &self,
        reference: &ExternalSecretReference,
    ) -> Result<SecretString, CredentialError> {
        let token = read_binding(&self.token)
            .await
            .map_err(|_| CredentialError::Unconfigured)?;
        self.reader
            .read_field(&token, reference.path(), reference.key())
            .await
            .map_err(|error| match error {
                VaultError::Missing => CredentialError::Missing,
                VaultError::Refused => CredentialError::Unconfigured,
                VaultError::Unavailable => CredentialError::Unavailable,
            })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use secrecy::ExposeSecret as _;
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

    /// A read sends the current token and namespace for the referenced path
    /// and key, and re-reads a rotated token on the next resolution.
    #[tokio::test]
    async fn kv_v2_reads_latest_version_with_current_token() {
        let server = MockServer::start().await;
        let token = token_file(TOKEN);
        let vault = backend(&server.uri(), &token);
        let target = reference("openai/a b#api_key");
        Mock::given(method("GET"))
            .and(path("/v1/kv/team/data/openai/a%20b"))
            .and(header("x-vault-token", TOKEN))
            .and(header("x-vault-namespace", "ns1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": {"data": {"api_key": "sk-v2"}, "metadata": {"version": 2}}
            })))
            .mount(&server)
            .await;
        let value = vault.fetch(&target).await.expect("latest version resolves");
        assert_eq!(value.expose_secret(), "sk-v2");

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

    /// The shared reader's outcome classes, an unreadable token source, and a
    /// blocked literal address map onto the closed credential result.
    #[tokio::test]
    async fn kv_v2_maps_outcomes() {
        let server = MockServer::start().await;
        let token = token_file(TOKEN);
        let vault = backend(&server.uri(), &token);
        let target = reference("openai/a b#api_key");
        for (status, expected) in [
            (404, CredentialError::Missing),
            (403, CredentialError::Unconfigured),
            (500, CredentialError::Unavailable),
        ] {
            server.reset().await;
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(status).set_body_json(json!({})))
                .mount(&server)
                .await;
            assert_eq!(
                vault.fetch(&target).await.map(|_| ()),
                Err(expected),
                "{status}"
            );
        }
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
        assert_eq!(
            backend("http://169.254.169.254", &token)
                .fetch(&target)
                .await
                .map(|_| ()),
            Err(CredentialError::Unconfigured)
        );
    }
}
