//! Operator connection credentials through the common stored-secret facade.
//!
//! Wyrd stores every Slack, PagerDuty, and HTTP credential in
//! `wyrd.operator_connections.secret_ciphertext` as the common envelope.
//! [`OperatorSecrets`] binds each envelope to its tenant, connection,
//! provider, name, and secret version, and seals, renews, and opens it
//! through the deployment's [`WyrdKms`]. Every key-service call runs with no
//! transaction or row lock held; writes are fenced on the exact row read.
//!
//! Failures leave this owner as [`WyrdError::OperatorKeyUnavailable`] with a
//! constant detail, or as a [`KmsError`] for delivery to classify. Logs carry
//! only the facade's bounded failure text, never keys or plaintext.

use std::sync::Arc;
use wyrd_kms::{KmsError, SecretContext, SecretPurpose, SecretScope, WyrdKms};
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{ConnectionName, OperatorConnectionId};
use wyrd_spec::operator_connection::{OperatorConnectionView, OperatorProvider};
use wyrd_sql::queries::operator_connections::{SealedSecret, StoredConnection};
use zeroize::Zeroizing;

/// The constant public detail of every key failure; the cause stays internal.
const UNAVAILABLE_DETAIL: &str = "stored-secret encryption could not seal or open the Operator credential; retry once [kms] is restored";

/// The stable unavailable refusal, logging `error` with bounded fields only.
fn unavailable(error: &KmsError) -> WyrdError {
    tracing::warn!(%error, "operator connection key unavailable");
    WyrdError::OperatorKeyUnavailable {
        message: UNAVAILABLE_DETAIL.to_owned(),
        details: serde_json::json!({}),
    }
}

/// The refusal of a credential write on a deployment without `[kms]`.
const UNCONFIGURED: KmsError = KmsError::Config {
    setting: "kms",
    reason: "is not configured",
};

/// Identity of one connection secret version: the trusted owner metadata
/// `(tenant, connection_id, provider, name, secret_version)` its envelope
/// context binds.
#[derive(Debug, Clone)]
pub struct SecretIdentity<'a> {
    /// Owning tenant.
    tenant: DataTenantId,
    /// Connection identity, the envelope owner.
    owner: String,
    /// Provider and name, the envelope authority.
    authority: [&'a str; 2],
    /// Secret version.
    secret_version: i32,
}

impl<'a> SecretIdentity<'a> {
    /// The identity of secret version `secret_version` of connection
    /// `connection_id`.
    #[must_use]
    pub fn new(
        tenant: DataTenantId,
        connection_id: OperatorConnectionId,
        provider: OperatorProvider,
        name: &'a ConnectionName,
        secret_version: i32,
    ) -> Self {
        Self {
            tenant,
            owner: connection_id.to_string(),
            authority: [provider.into(), name.as_str()],
            secret_version,
        }
    }

    /// The identity of `view`'s secret version `secret_version`.
    #[must_use]
    pub fn of(tenant: DataTenantId, view: &'a OperatorConnectionView, secret_version: i32) -> Self {
        Self::new(
            tenant,
            view.connection_id,
            view.config.provider(),
            &view.name,
            secret_version,
        )
    }

    /// The envelope context this identity binds.
    fn context(&self) -> SecretContext<'_> {
        SecretContext {
            scope: SecretScope::Tenant(self.tenant),
            purpose: SecretPurpose::OperatorConnection,
            owner: &self.owner,
            authority: &self.authority,
            version: Some(i64::from(self.secret_version)),
        }
    }
}

/// Owner of Operator credential sealing, renewal, and opening.
///
/// Cheap to clone; clones share the deployment facade. `None` means the
/// deployment has no `[kms]`: writes refuse and deliveries stay retryable.
#[derive(Debug, Clone, Default)]
pub struct OperatorSecrets {
    /// The deployment facade, when `[kms]` is configured.
    kms: Option<Arc<WyrdKms>>,
}

impl OperatorSecrets {
    /// Build the owner over the deployment facade.
    #[must_use]
    pub const fn new(kms: Option<Arc<WyrdKms>>) -> Self {
        Self { kms }
    }

    /// Seal `plaintext` as the secret version `identity` names.
    ///
    /// Callers run this before opening the transaction that stores it.
    ///
    /// # Errors
    /// Returns [`WyrdError::OperatorKeyUnavailable`] with a constant detail
    /// when no `[kms]` is configured or sealing fails.
    pub async fn seal(
        &self,
        identity: &SecretIdentity<'_>,
        plaintext: &[u8],
    ) -> Result<SealedSecret, WyrdError> {
        let kms = self
            .kms
            .as_deref()
            .ok_or_else(|| unavailable(&UNCONFIGURED))?;
        kms.seal(&identity.context(), plaintext)
            .await
            .map(|envelope| SealedSecret { envelope })
            .map_err(|error| unavailable(&error))
    }

    /// Renew the encryption of `stored`'s current secret: open it and reseal
    /// the same value under a fresh DEK and nonce on the same secret version.
    ///
    /// Callers run this with no transaction open and fence the swap on the
    /// row they read.
    ///
    /// # Errors
    /// Returns [`WyrdError::OperatorKeyUnavailable`] with a constant detail
    /// when no `[kms]` is configured or the envelope cannot open or reseal.
    pub async fn renew(
        &self,
        tenant: DataTenantId,
        stored: &StoredConnection,
    ) -> Result<SealedSecret, WyrdError> {
        let kms = self
            .kms
            .as_deref()
            .ok_or_else(|| unavailable(&UNCONFIGURED))?;
        let identity = SecretIdentity::of(tenant, &stored.view, stored.secret_version);
        kms.renew(&identity.context(), &stored.sealed.envelope)
            .await
            .map(|envelope| SealedSecret { envelope })
            .map_err(|error| unavailable(&error))
    }

    /// Open the current secret of `stored` for one delivery attempt.
    ///
    /// # Errors
    /// Returns the facade's [`KmsError`], or a [`KmsError::Config`] when no
    /// `[kms]` is configured.
    pub async fn open(
        &self,
        tenant: DataTenantId,
        stored: &StoredConnection,
    ) -> Result<Zeroizing<Vec<u8>>, KmsError> {
        let kms = self.kms.as_deref().ok_or(UNCONFIGURED)?;
        let identity = SecretIdentity::of(tenant, &stored.view, stored.secret_version);
        kms.open(&identity.context(), &stored.sealed.envelope).await
    }
}
