//! Stored-secret rewrap: move every long-lived auth and Gateway envelope to
//! the active wrapping key.
//!
//! The pass covers tenant human-connection and workload-issuer client secrets,
//! the platform connection secret, and gateway `managed_secret` provider keys,
//! so `remaining == 0` speaks for every envelope those owners store.
//!
//! Rotation procedure (operator runbook in `docs/`): configure the new key as
//! `[kms].active_key`, keep the old key in `retained_keys`, and roll every
//! replica. Each boot runs [`SealedSecretRewrap::run`], which rewraps every
//! envelope that does not name the active key. A report taken while any
//! replica still writes with the old key proves nothing: that replica can
//! seal a new secret under it after the pass counted zero. The old key may be
//! removed only after every serving writer uses the new one and a
//! verification pass started after that point reports `remaining == 0`.
//!
//! Rewrap moves only the wrapped data key; the value is never decrypted. Each
//! envelope's context is rebuilt from its row's trusted owner metadata, so an
//! envelope copied to another owner fails to unwrap and stays `remaining`.
//! Key-service calls run with no transaction or row lock held; each value is
//! replaced by a compare-and-swap on the exact bytes read, so a concurrent
//! replacement or a second replica's rewrap is never overwritten; the value
//! it lost to counts as `remaining` and the next pass rewraps it. It is an
//! engine-internal transition that evaluates no principal permission, so it
//! logs counts (never keys or plaintext) and writes no audit.
//!
//! A deployment without `[kms]` runs the same pass with no facade: every
//! stored envelope is then `remaining`, which is how boot proves such a
//! deployment holds no secret it could not open.

use std::future::Future;
use std::sync::Arc;

use uuid::Uuid;
use wyrd_kms::{SecretContext, SecretPurpose, SecretScope, WyrdKms};
use wyrd_spec::DataTenantId;
use wyrd_sql::queries::auth::{
    SealedSecretTable, sealed_tenant_secrets, swap_sealed_tenant_secret,
};
use wyrd_sql::queries::platform::identity::{platform_sealed_secret, swap_platform_sealed_secret};
use wyrd_sql::{OperatorPool, SqlError};

/// Outcome of one rewrap pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RewrapReport {
    /// Envelopes moved to the active key by this pass.
    pub rewrapped: usize,
    /// Envelopes already under the active key.
    pub current: usize,
    /// Envelopes still not under the active key: they do not unwrap under
    /// their owner, the key service failed, or they changed concurrently. The old key must not be retired while this
    /// is nonzero, nor on the strength of a pass that ran before every writer
    /// moved to the write key.
    pub remaining: usize,
}

/// Owner of the cross-tenant stored-secret rewrap.
#[derive(Clone)]
pub struct SealedSecretRewrap {
    /// Cross-tenant operator pool; the pass reads and swaps every tenant.
    operator: OperatorPool,
    /// Deployment facade; `None` without `[kms]`, where every envelope
    /// counts as `remaining`.
    kms: Option<Arc<WyrdKms>>,
}

impl SealedSecretRewrap {
    /// Build the rewrap owner over the operator pool and deployment facade.
    #[must_use]
    pub fn new(operator: OperatorPool, kms: Option<Arc<WyrdKms>>) -> Self {
        Self { operator, kms }
    }

    /// Rewrap every tenant and platform provider secret not under the active
    /// key.
    ///
    /// Walks human connections, workload trusted issuers, Gateway managed
    /// keys, and the platform connection. An envelope that does not unwrap
    /// under its owner, or one whose swap loses to a concurrent write, is
    /// counted in `remaining`; the former is logged with its table and
    /// tenant, never its bytes.
    ///
    /// # Errors
    /// Returns [`SqlError`] when a read or swap statement fails; rows already
    /// swapped stay swapped, and a rerun resumes from the rest.
    pub async fn run(&self) -> Result<RewrapReport, SqlError> {
        let mut report = RewrapReport::default();
        let operator = &self.operator;
        for table in SealedSecretTable::ALL {
            for row in &sealed_tenant_secrets(operator, table).await? {
                let authority: Vec<&str> = row.authority.iter().map(String::as_str).collect();
                let scope = DataTenantId::try_from(row.data_tenant_id).map(SecretScope::Tenant);
                self.settle(
                    &mut report,
                    &row.client_secret_enc,
                    table.label(),
                    Some(row.data_tenant_id),
                    scope.ok().map(|scope| SecretContext {
                        scope,
                        purpose: purpose(table),
                        owner: &row.owner,
                        authority: &authority,
                        version: None,
                    }),
                    |bytes| async move {
                        swap_sealed_tenant_secret(operator, table, row, &bytes).await
                    },
                )
                .await?;
            }
        }
        if let Some(row) = &platform_sealed_secret(operator).await? {
            let previous = &row.client_secret_enc;
            self.settle(
                &mut report,
                previous,
                "platform.oidc_connection",
                None,
                Some(SecretContext {
                    scope: SecretScope::Platform,
                    purpose: SecretPurpose::PlatformOidc,
                    owner: &row.issuer_url,
                    authority: &[&row.client_id, &row.client_auth],
                    version: None,
                }),
                |bytes| async move {
                    swap_platform_sealed_secret(operator, previous, &bytes).await
                },
            )
            .await?;
        }
        tracing::info!(
            backend = ?self.kms.as_ref().map(|kms| kms.backend()),
            rewrapped = report.rewrapped,
            current = report.current,
            remaining = report.remaining,
            "stored secret rewrap finished"
        );
        Ok(report)
    }

    /// Classify one envelope, swap in its rewrap when needed, and count the
    /// outcome in `report`.
    ///
    /// `context` is the envelope's expected owner, `None` when its row's
    /// owner metadata is itself invalid. `swap` receives the rewrapped bytes
    /// and reports whether its compare-and-swap won; a lost swap means the
    /// row changed concurrently and stays `remaining` for the next pass.
    ///
    /// # Errors
    /// Returns [`SqlError`] when `swap` fails.
    async fn settle<F>(
        &self,
        report: &mut RewrapReport,
        sealed: &[u8],
        table: &str,
        tenant_id: Option<Uuid>,
        context: Option<SecretContext<'_>>,
        swap: impl FnOnce(Vec<u8>) -> F,
    ) -> Result<(), SqlError>
    where
        F: Future<Output = Result<bool, SqlError>>,
    {
        let (Some(kms), Some(context)) = (self.kms.as_deref(), context) else {
            tracing::error!(table, tenant_id = ?tenant_id, "stored secret cannot be rewrapped without [kms] and valid owner metadata");
            report.remaining += 1;
            return Ok(());
        };
        let rewrapped = match kms.is_current(sealed) {
            Ok(true) => {
                report.current += 1;
                return Ok(());
            }
            Ok(false) => kms.rewrap(&context, sealed).await,
            Err(error) => Err(error),
        };
        match rewrapped {
            Ok(bytes) => {
                if swap(bytes).await? {
                    report.rewrapped += 1;
                } else {
                    report.remaining += 1;
                }
            }
            Err(error) => {
                tracing::error!(table, tenant_id = ?tenant_id, %error, "stored secret could not be rewrapped");
                report.remaining += 1;
            }
        }
        Ok(())
    }
}

/// The envelope purpose a tenant column's secrets are bound to.
const fn purpose(table: SealedSecretTable) -> SecretPurpose {
    match table {
        SealedSecretTable::HumanConnections => SecretPurpose::AuthHumanConnection,
        SealedSecretTable::TrustedIssuers => SecretPurpose::AuthTrustedIssuer,
        SealedSecretTable::GatewayProviderCredentials => SecretPurpose::GatewayProviderCredential,
    }
}
