//! Provider-credential mutation, kept off the gateway administration handle.
//!
//! Creating, rotating, revoking, and deleting a provider credential all live
//! here rather than on [`crate::gateway::Gateway`], because none of them can
//! be offered to a first-class SDK without also offering ManagedSecret
//! mutation. Submission is obvious: the write body can hold the secret itself
//! in [`ProviderCredentialWriteSource::ManagedSecret`]. Revocation and
//! deletion are the same boundary for a less obvious reason — they name a
//! credential and nothing else, so a caller revoking `primary` cannot be told
//! apart from a caller revoking a managed secret without a second read or a
//! new server distinction, and the approved contract adds neither.
//!
//! The restriction is therefore structural. `wyrd-sdk-rust` does not
//! re-export this module, and the Python and TypeScript bindings never
//! construct [`CredentialWriter`], so no first-class SDK can reach provider
//! credential mutation at all and none of them needs a runtime source check.
//! An SDK caller reads redacted credentials, administers deployments and
//! policies, and invokes the gateway. The CLI and the scoped MCP tool
//! construct this handle and remain the supported mutation surfaces.
//!
//! [`ProviderCredentialWriteSource::ManagedSecret`]: wyrd_spec::gateway::ProviderCredentialWriteSource::ManagedSecret

use reqwest::Method;
use serde::Serialize;
use serde::de::DeserializeOwned;
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::{ProviderCredentialView, ProviderCredentialWrite};
use wyrd_spec::ids::ProviderCredentialName;

use crate::WyrdClient;
use crate::gateway::credential_path;

/// Mutates provider credentials over one shared [`WyrdClient`].
///
/// Cloning is cheap; the underlying client shares its connection pool and
/// token cache. The handle holds no submitted value: a write body travels
/// straight to the server, which seals a managed secret before it is stored
/// and answers with the redacted view.
#[derive(Debug, Clone)]
pub struct CredentialWriter {
    /// Authenticated transport used for every mutation.
    client: WyrdClient,
}

impl CredentialWriter {
    /// Binds the handle to an assembled client.
    #[must_use]
    pub fn new(client: WyrdClient) -> Self {
        Self { client }
    }

    /// Creates or rotates a provider credential and returns its redacted view.
    ///
    /// A repeated submission under the same name rotates the credential in
    /// place. The answer never echoes a submitted secret.
    ///
    /// # Errors
    /// Returns the server's stable permission, invalid-configuration,
    /// conflict, or availability error.
    pub async fn put_credential(
        &self,
        write: &ProviderCredentialWrite,
    ) -> Result<ProviderCredentialView, WyrdError> {
        self.send(Method::PUT, &credential_path(&write.name), Some(write))
            .await
    }

    /// Terminally revokes a provider credential; repeating is harmless.
    ///
    /// # Errors
    /// Returns the server's permission, not-found, or availability error.
    pub async fn revoke_credential(
        &self,
        name: &ProviderCredentialName,
    ) -> Result<ProviderCredentialView, WyrdError> {
        let path = format!("{}/revoke", credential_path(name));
        self.send(Method::POST, &path, None::<&()>).await
    }

    /// Deletes an unreferenced provider credential; an absent name succeeds.
    ///
    /// # Errors
    /// Returns the server's permission, conflict (still referenced), or
    /// availability error.
    pub async fn delete_credential(&self, name: &ProviderCredentialName) -> Result<(), WyrdError> {
        self.send(Method::DELETE, &credential_path(name), None::<&()>)
            .await
    }

    /// Issues one mutation against the shared client and decodes its answer.
    ///
    /// # Errors
    /// Returns the server's stable error or a transport/decoding failure.
    async fn send<S: Serialize, D: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&S>,
    ) -> Result<D, WyrdError> {
        self.client.request_json(method, path, body).await
    }
}
