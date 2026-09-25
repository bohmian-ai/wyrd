//! Sealing-key rotation: rewrap every stored provider secret under the write key.
//!
//! Rotation procedure (operator runbook in `docs/`): configure the new key K2
//! as the write key, keep the old key K1 in the retained set, and roll every
//! replica. Each boot runs [`SealedSecretRewrap::run`], which reseals every
//! ciphertext the write key did not produce. A report taken while any replica
//! still writes with K1 proves nothing: that replica can seal a new secret
//! under K1 after the pass counted zero. K1 may be removed only after every
//! serving writer uses K2 and a verification pass started after that point —
//! for example, one more replica restart — reports `remaining == 0`. That pass
//! also reseals any late K1 write, so it is both the proof and the repair.
//!
//! The pass is idempotent and safe across replicas: each row is replaced by a
//! compare-and-swap on the exact bytes read, so a concurrent reconfiguration or
//! a second replica's rewrap is never overwritten. It is an engine-internal
//! transition that evaluates no principal permission, so it logs counts (never
//! keys or plaintext) and writes no audit.
//!
//! A keyless deployment runs the same pass with no keyring: every stored
//! ciphertext is then `remaining`, which is how boot proves a keyless
//! deployment holds no secret it could not open.

use std::future::Future;
use std::sync::Arc;

use uuid::Uuid;
use wyrd_crypt::SealingKeyring;
use wyrd_sql::queries::auth::{
    SealedSecretTable, sealed_tenant_secrets, swap_sealed_tenant_secret,
};
use wyrd_sql::queries::platform::identity::{platform_sealed_secret, swap_platform_sealed_secret};
use wyrd_sql::{OperatorPool, SqlError};

/// Outcome of one rewrap pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RewrapReport {
    /// Ciphertexts resealed under the write key by this pass.
    pub rewrapped: usize,
    /// Ciphertexts already under the write key.
    pub current: usize,
    /// Ciphertexts still not under the write key: no held key opens them, or
    /// they changed concurrently. The old key must not be retired while this
    /// is nonzero, nor on the strength of a pass that ran before every writer
    /// moved to the write key.
    pub remaining: usize,
}

/// Owner of the cross-tenant sealed-secret rewrap.
#[derive(Clone)]
pub struct SealedSecretRewrap {
    /// Cross-tenant operator pool; the pass reads and swaps every tenant.
    operator: OperatorPool,
    /// Deployment keyring; `None` on a keyless deployment, where every
    /// ciphertext counts as `remaining`.
    keyring: Option<Arc<SealingKeyring>>,
}

impl SealedSecretRewrap {
    /// Build the rewrap owner over the operator pool and deployment keyring.
    #[must_use]
    pub fn new(operator: OperatorPool, keyring: Option<Arc<SealingKeyring>>) -> Self {
        Self { operator, keyring }
    }

    /// Reseal every tenant and platform provider secret not under the write key.
    ///
    /// Walks human connections, workload trusted issuers, and the platform
    /// connection. A ciphertext no held key opens is counted in `remaining`
    /// and logged with its table and tenant, never its bytes.
    ///
    /// # Errors
    /// Returns [`SqlError`] when a read or swap statement fails; rows already
    /// swapped stay swapped, and a rerun resumes from the rest.
    pub async fn run(&self) -> Result<RewrapReport, SqlError> {
        let mut report = RewrapReport::default();
        let operator = &self.operator;
        for table in SealedSecretTable::ALL {
            for row in &sealed_tenant_secrets(operator, table).await? {
                self.settle(
                    &mut report,
                    &row.client_secret_enc,
                    table.label(),
                    Some(row.data_tenant_id),
                    |bytes| async move {
                        swap_sealed_tenant_secret(operator, table, row, &bytes).await
                    },
                )
                .await?;
            }
        }
        if let Some(previous) = &platform_sealed_secret(operator).await? {
            self.settle(
                &mut report,
                previous,
                "platform.oidc_connection",
                None,
                |bytes| async move {
                    swap_platform_sealed_secret(operator, previous, &bytes).await
                },
            )
            .await?;
        }
        tracing::info!(
            write_key = ?self.keyring.as_ref().map(|keyring| keyring.write_key_id()),
            rewrapped = report.rewrapped,
            current = report.current,
            remaining = report.remaining,
            "sealed provider secret rewrap finished"
        );
        Ok(report)
    }

    /// Classify one ciphertext, swap in its reseal when needed, and count the
    /// outcome in `report`.
    ///
    /// `swap` receives the resealed bytes and reports whether its
    /// compare-and-swap won; a lost swap means the row changed concurrently
    /// and stays `remaining` for the next pass.
    ///
    /// # Errors
    /// Returns [`SqlError`] when `swap` fails.
    async fn settle<F>(
        &self,
        report: &mut RewrapReport,
        sealed: &[u8],
        table: &str,
        tenant_id: Option<Uuid>,
        swap: impl FnOnce(Vec<u8>) -> F,
    ) -> Result<(), SqlError>
    where
        F: Future<Output = Result<bool, SqlError>>,
    {
        match self.reseal(sealed) {
            Reseal::Current => report.current += 1,
            Reseal::Unopenable => {
                tracing::error!(
                    table,
                    tenant_id = ?tenant_id,
                    "sealed secret is not openable by any held sealing key"
                );
                report.remaining += 1;
            }
            Reseal::Rewrapped(bytes) => {
                if swap(bytes).await? {
                    report.rewrapped += 1;
                } else {
                    report.remaining += 1;
                }
            }
        }
        Ok(())
    }

    /// Classify one ciphertext and reseal it when needed; without a keyring
    /// nothing opens.
    fn reseal(&self, sealed: &[u8]) -> Reseal {
        let Some(keyring) = &self.keyring else {
            return Reseal::Unopenable;
        };
        match keyring.rewrap(sealed) {
            Ok(None) => Reseal::Current,
            Ok(Some(bytes)) => Reseal::Rewrapped(bytes),
            Err(_) => Reseal::Unopenable,
        }
    }
}

/// What one ciphertext needs.
enum Reseal {
    /// Already sealed under the write key.
    Current,
    /// Resealed under the write key; must be swapped in.
    Rewrapped(Vec<u8>),
    /// No held key opens it.
    Unopenable,
}
