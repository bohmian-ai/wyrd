//! Deployment initialization: establish the platform administrative root.
//!
//! This is the one-time operation that gives a deployment an identity to
//! administer it with. It replaces the removed per-tenant key-minting path,
//! which could only mint a credential for a tenant that already existed —
//! leaving the first tenant to be created by hand.
//!
//! Initialization is an operator-invoked subcommand rather than a server-start
//! side effect, so the root credential is printed to the operator's terminal
//! instead of the server's log pipeline. Server start creates no administrative
//! state and emits no credential material under any deployment profile.

use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;
use wyrd_auth::platform_credentials::{PlatformCredential, PlatformCredentialError};
use wyrd_auth_issue::hash_api_key;
use wyrd_runtime::{Permission, PermissionSet};
use wyrd_spec::auth::PrincipalKindTag;
use wyrd_sql::queries::platform::credentials::insert_platform_credential_tx;
use wyrd_sql::queries::platform::principal_grants::set_platform_grant_tx;
use wyrd_sql::queries::platform::principals::{
    insert_platform_principal_tx, platform_principal_id_by_name,
};
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
    /// Credential issuance failed.
    #[error("initial credential could not be issued: {0}")]
    Credential(#[from] PlatformCredentialError),
    /// A platform store write failed.
    #[error("platform store write failed: {0}")]
    Store(#[from] SqlError),
    /// The credential could not be disclosed to the operator.
    #[error("initial credential could not be disclosed: {0}")]
    Disclose(#[from] std::io::Error),
}

/// The fixed authority every platform administrator holds.
///
/// One set, not one per administrator: platform authority is not editable, so
/// the root established at initialization and a human registered against the
/// OIDC connection later hold exactly this and are distinguishable only by how
/// they authenticate.
///
/// Narrow on purpose: the platform plane manages tenant lifecycle, recovers
/// tenant administration, and configures who may sign in to administer the
/// platform. It grants no access to any tenant's resources, so this set names
/// the tenant directory and the platform's own identity, and nothing inside a
/// tenant.
///
/// Platform identity is here because the root is the only identity a fresh
/// deployment has: without it, nobody could ever configure the OIDC connection
/// or register the first human administrator, and federated login would be
/// unreachable by construction.
pub(crate) fn platform_administrator_grant() -> PermissionSet {
    let mut grant = PermissionSet::new();
    grant.insert(Permission::tenant_create());
    grant.insert(Permission::tenant_read());
    grant.insert(Permission::tenant_suspend());
    grant.insert(Permission::tenant_recover_admin());
    grant.insert(Permission::platform_identity_read());
    grant.insert(Permission::platform_identity_write());
    grant.insert(Permission::platform_credential_read());
    grant.insert(Permission::platform_credential_write());
    grant
}

/// Establish the deployment's administrative root, disclosing its credential.
///
/// Creates the platform principal, grants it the platform-plane authority
/// above, issues its first credential, and writes the plaintext to
/// `disclosure` exactly once. The plaintext is never persisted, logged,
/// traced, or returned.
///
/// Disclosure is a step of the transaction, not something the caller does
/// afterwards. The write and its flush both happen while the transaction is
/// still open, so a closed or failing writer drops the transaction and leaves
/// the deployment uninitialized and retryable instead of leaving a durable
/// root whose only credential nobody ever read. The reverse ordering is not
/// attainable across two systems: a commit that fails after a successful
/// disclosure leaves the operator holding a secret that was never stored,
/// which costs them one retry rather than the deployment.
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
/// [`InitError::Disclose`] when the credential cannot be written or flushed,
/// and [`InitError::Store`] when a write or the commit fails. A failed
/// initialization commits nothing and can be retried unchanged.
#[tracing::instrument(level = "info", skip(pool, disclosure), err)]
pub async fn initialize_platform_root(
    pool: &OperatorPool,
    disclosure: &mut dyn std::io::Write,
) -> Result<(), InitError> {
    let principal_id = Uuid::now_v7();
    let credential = PlatformCredential::generate();
    let raw = credential.secret.clone();
    let secret_hash = tokio::task::spawn_blocking(move || hash_api_key(&raw))
        .await
        .map_err(|error| InitError::Credential(PlatformCredentialError::Join(error)))?
        .map_err(|error| InitError::Credential(PlatformCredentialError::Hash(error)))?;

    let mut conn = pool.begin_platform_audited().await?;

    match insert_platform_principal_tx(
        &mut conn,
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
            drop(conn);
            return Err(InitError::AlreadyInitialized);
        }
        Err(error) => {
            drop(conn);
            return Err(InitError::Store(error));
        }
    }

    let grant = serde_json::to_value(platform_administrator_grant().iter().collect::<Vec<_>>())
        .expect("permission set serializes to JSON");
    set_platform_grant_tx(&mut conn, principal_id, &grant).await?;
    insert_platform_credential_tx(
        &mut conn,
        Uuid::new_v4(),
        principal_id,
        &credential.prefix,
        &secret_hash,
        None,
    )
    .await?;

    match disclose_credential(disclosure, &credential.secret) {
        Ok(()) => {}
        Err(error) => {
            drop(conn);
            return Err(InitError::Disclose(error));
        }
    }

    conn.commit().await?;
    Ok(())
}

/// Write the one disclosure of a freshly minted root credential and flush it.
///
/// Separate from the workflow above so the ordering it protects is legible:
/// every byte, including the flush that proves the operator's terminal
/// actually took them, must succeed before the transaction may commit.
///
/// # Errors
/// Returns the underlying [`std::io::Error`] when any write or the flush
/// fails.
fn disclose_credential(
    disclosure: &mut dyn std::io::Write,
    secret: &SecretString,
) -> std::io::Result<()> {
    writeln!(disclosure, "Wyrd initialization complete.")?;
    writeln!(disclosure, "Platform administrative credential:")?;
    writeln!(disclosure, "{}", secret.expose_secret())?;
    writeln!(
        disclosure,
        "Store this credential securely. It cannot be retrieved again."
    )?;
    disclosure.flush()
}

/// Failure while reissuing the administrative root's credential.
#[derive(Debug, thiserror::Error)]
pub enum RecoverRootError {
    /// The deployment has no administrative root to issue for.
    #[error("this deployment is not initialized; run `wyrd-server init` first")]
    NotInitialized,
    /// Credential issuance failed.
    #[error("replacement credential could not be issued: {0}")]
    Credential(#[from] PlatformCredentialError),
    /// A platform store write failed.
    #[error("platform store write failed: {0}")]
    Store(#[from] SqlError),
}

/// Issue a replacement credential for the existing administrative root.
///
/// The recovery of last resort. A platform credential cannot be recovered — only
/// its verifier is stored — so losing every one of them would otherwise leave a
/// deployment permanently unadministrable: the platform routes need a platform
/// session, and every way to obtain one needs a credential that no longer
/// exists. Reissuing is available only to whoever already holds the deployment's
/// database access, which is the same authority that ran initialization.
///
/// Deliberately not an HTTP route, and deliberately not a second `init`: it
/// creates no principal, writes no grant, and reads the root by the fixed name
/// initialization gave it, so the identity, its authority, and its audit history
/// all stay the ones the deployment already had. Credentials this root already
/// holds are left alone — retiring them is a separate, auditable platform
/// operation, and a recovery that silently revoked them would cut off an
/// operator who still had one.
///
/// # Errors
/// Returns [`RecoverRootError::NotInitialized`] when no root exists,
/// [`RecoverRootError::Credential`] when the credential cannot be generated or
/// hashed, and [`RecoverRootError::Store`] when a write or the commit fails. A
/// failed attempt commits nothing and can be retried unchanged.
#[tracing::instrument(level = "info", skip(pool), err)]
pub async fn issue_platform_root_credential(
    pool: &OperatorPool,
) -> Result<SecretString, RecoverRootError> {
    let credential = PlatformCredential::generate();
    let raw = credential.secret.clone();
    let secret_hash = tokio::task::spawn_blocking(move || hash_api_key(&raw))
        .await
        .map_err(|error| RecoverRootError::Credential(PlatformCredentialError::Join(error)))?
        .map_err(|error| RecoverRootError::Credential(PlatformCredentialError::Hash(error)))?;

    let mut conn = pool.begin_platform_audited().await?;
    let Some(principal_id) = platform_principal_id_by_name(&mut conn, PLATFORM_ROOT_NAME).await?
    else {
        drop(conn);
        return Err(RecoverRootError::NotInitialized);
    };
    insert_platform_credential_tx(
        &mut conn,
        Uuid::new_v4(),
        principal_id,
        &credential.prefix,
        &secret_hash,
        None,
    )
    .await?;
    conn.commit().await?;
    Ok(credential.secret)
}

/// The shape of the grant the administrative root is initialized with.
#[cfg(test)]
mod tests {
    use super::platform_administrator_grant;
    use wyrd_runtime::Permission;

    /// The administrative root can manage tenant lifecycle and recover tenant
    /// administration, and nothing inside a tenant.
    #[test]
    fn the_root_grant_covers_the_platform_plane_only() {
        let grant = platform_administrator_grant();

        for required in [
            Permission::tenant_create(),
            Permission::tenant_read(),
            Permission::tenant_suspend(),
            Permission::tenant_recover_admin(),
            Permission::platform_identity_read(),
            Permission::platform_identity_write(),
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
