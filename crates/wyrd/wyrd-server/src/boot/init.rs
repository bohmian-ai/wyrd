//! Deployment initialization: establish the platform administrative root.
//!
//! This is the one-time operation that gives a deployment an identity to
//! administer it with. It replaces the removed `bootstrap-key` path, which
//! could only mint a credential for a tenant that already existed — leaving the
//! first tenant to be created by hand.
//!
//! Initialization is an operator-invoked subcommand rather than a server-start
//! side effect, so the root credential is printed to the operator's terminal
//! instead of the server's log pipeline. Server start creates no administrative
//! state and emits no credential material under any deployment profile.

use secrecy::SecretString;
use uuid::Uuid;
use wyrd_auth::platform_credentials::{PlatformCredential, PlatformCredentialError};
use wyrd_auth_issue::hash_api_key;
use wyrd_runtime::{Permission, PermissionSet};
use wyrd_spec::auth::PrincipalKindTag;
use wyrd_sql::queries::platform::credentials::insert_platform_credential_tx;
use wyrd_sql::queries::platform::principal_grants::set_platform_grant_tx;
use wyrd_sql::queries::platform::principals::insert_platform_principal_tx;
use wyrd_sql::{OperatorPool, SqlError};

/// Operator-facing name of the deployment's administrative root.
///
/// Fixed rather than caller-supplied: the unique constraint on this name is
/// what makes initialization single, so a second invocation — concurrent or
/// later — collides instead of minting a second root.
pub const PLATFORM_ROOT_NAME: &str = "platform-admin";

/// Failure while initializing the deployment's administrative root.
#[derive(Debug, thiserror::Error)]
pub enum InitError {
    /// The deployment already has its administrative root.
    #[error("this deployment is already initialized")]
    AlreadyInitialized,
    /// The platform control plane has no operator connection configured.
    #[error("platform control plane is not configured; set the platform-admin DSN")]
    NotConfigured,
    /// Credential issuance failed.
    #[error("initial credential could not be issued: {0}")]
    Credential(#[from] PlatformCredentialError),
    /// A platform store write failed.
    #[error("platform store write failed: {0}")]
    Store(#[from] SqlError),
}

/// The authority a freshly initialized administrative root holds.
///
/// Narrow on purpose: the platform plane manages tenant lifecycle and recovers
/// tenant administration. It grants no access to any tenant's resources, so
/// this set names the tenant directory and nothing inside a tenant.
fn platform_root_grant() -> PermissionSet {
    let mut grant = PermissionSet::new();
    grant.insert(Permission::tenant_create());
    grant.insert(Permission::tenant_read());
    grant.insert(Permission::tenant_suspend());
    grant.insert(Permission::tenant_recover_admin());
    grant
}

/// Establish the deployment's administrative root and return its credential.
///
/// Creates the platform principal, grants it the platform-plane authority
/// above, and issues its first credential, returning the plaintext for a single
/// print. The plaintext is never persisted, logged, or traced.
///
/// All three writes commit together. That is not tidiness: the principal's name
/// is unique, so a partial initialization would leave a root with no credential
/// that no later attempt could replace — the deployment would be permanently
/// unadministrable, with no way back except the out-of-band SQL this operation
/// exists to abolish. One transaction makes every failure leave the deployment
/// uninitialized and retryable.
///
/// Single initialization is enforced durably by the unique principal name
/// rather than by a read-then-write check, so two operators racing produce one
/// root and one refusal rather than two roots.
///
/// # Errors
/// Returns [`InitError::AlreadyInitialized`] when a root already exists,
/// [`InitError::Credential`] when the credential cannot be generated or hashed,
/// and [`InitError::Store`] when a write or the commit fails. A failed
/// initialization commits nothing and can be retried unchanged.
#[tracing::instrument(level = "info", skip(pool), err)]
pub async fn initialize_platform_root(pool: &OperatorPool) -> Result<SecretString, InitError> {
    let principal_id = Uuid::now_v7();
    let credential = PlatformCredential::generate();
    let raw = credential.secret.clone();
    let secret_hash = tokio::task::spawn_blocking(move || hash_api_key(&raw))
        .await
        .map_err(|error| InitError::Credential(PlatformCredentialError::Join(error)))?
        .map_err(|error| InitError::Credential(PlatformCredentialError::Hash(error)))?;

    let mut tx = pool.begin().await.map_err(SqlError::from)?;

    match insert_platform_principal_tx(
        &mut tx,
        principal_id,
        PrincipalKindTag::GlobalAdmin,
        PLATFORM_ROOT_NAME,
    )
    .await
    {
        Ok(()) => {}
        Err(SqlError::UniqueViolation { .. }) => {
            // Nothing was written, so the rollback is a formality; it matters
            // that we do not leave the connection holding an open transaction.
            let _ = tx.rollback().await;
            return Err(InitError::AlreadyInitialized);
        }
        Err(error) => {
            let _ = tx.rollback().await;
            return Err(InitError::Store(error));
        }
    }

    let grant = serde_json::to_value(platform_root_grant().iter().collect::<Vec<_>>())
        .expect("permission set serializes to JSON");
    set_platform_grant_tx(&mut tx, principal_id, &grant).await?;
    insert_platform_credential_tx(
        &mut tx,
        Uuid::new_v4(),
        principal_id,
        &credential.prefix,
        &secret_hash,
        None,
    )
    .await?;

    tx.commit().await.map_err(SqlError::from)?;
    Ok(credential.secret)
}

#[cfg(test)]
mod tests {
    use super::platform_root_grant;
    use wyrd_runtime::Permission;

    /// The administrative root can manage tenant lifecycle and recover tenant
    /// administration, and nothing inside a tenant.
    #[test]
    fn the_root_grant_covers_the_platform_plane_only() {
        let grant = platform_root_grant();

        for required in [
            Permission::tenant_create(),
            Permission::tenant_read(),
            Permission::tenant_suspend(),
            Permission::tenant_recover_admin(),
        ] {
            assert!(grant.contains(&required), "{required:?} is granted");
        }
        for forbidden in [
            Permission::card_read(),
            Permission::card_write(),
            Permission::artifact_read(),
            Permission::users_manage(),
        ] {
            assert!(
                !grant.contains(&forbidden),
                "{forbidden:?} is tenant authority the platform root must not hold"
            );
        }
    }
}
