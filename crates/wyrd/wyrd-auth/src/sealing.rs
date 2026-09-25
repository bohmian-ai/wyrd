//! Sealing-key rotation: rewrap every stored provider secret under the write key.
//!
//! Rotation procedure (operator runbook in `docs/`): configure the new key as
//! the write key and keep the old key in the retained set; boot runs
//! [`SealedSecretRewrap::run`], which reseals every ciphertext the write key did
//! not produce; once a report shows `remaining == 0` on every replica's boot,
//! the old key is no longer referenced and can be removed from the retained
//! set.
//!
//! The pass is idempotent and safe across replicas: each row is replaced by a
//! compare-and-swap on the exact bytes read, so a concurrent reconfiguration or
//! a second replica's rewrap is never overwritten. It is an engine-internal
//! transition that evaluates no principal permission, so it logs counts (never
//! keys or plaintext) and writes no audit.

use std::sync::Arc;

use wyrd_crypt::SealingKeyring;
use wyrd_sql::queries::auth::{SealedSecretTable, sealed_tenant_secrets, swap_sealed_tenant_secret};
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
    /// is nonzero.
    pub remaining: usize,
}

/// Owner of the cross-tenant sealed-secret rewrap.
#[derive(Clone)]
pub struct SealedSecretRewrap {
    operator: OperatorPool,
    keyring: Arc<SealingKeyring>,
}

impl SealedSecretRewrap {
    /// Build the rewrap owner over the operator pool and deployment keyring.
    #[must_use]
    pub fn new(operator: OperatorPool, keyring: Arc<SealingKeyring>) -> Self {
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
        for table in SealedSecretTable::ALL {
            for row in sealed_tenant_secrets(&self.operator, table).await? {
                match self.reseal(&row.client_secret_enc) {
                    Reseal::Current => report.current += 1,
                    Reseal::Unopenable => {
                        tracing::error!(
                            table = table.label(),
                            tenant_id = %row.data_tenant_id,
                            "sealed secret is not openable by any held sealing key"
                        );
                        report.remaining += 1;
                    }
                    Reseal::Rewrapped(bytes) => {
                        if swap_sealed_tenant_secret(&self.operator, table, &row, &bytes).await? {
                            report.rewrapped += 1;
                        } else {
                            report.remaining += 1;
                        }
                    }
                }
            }
        }
        if let Some(previous) = platform_sealed_secret(&self.operator).await? {
            match self.reseal(&previous) {
                Reseal::Current => report.current += 1,
                Reseal::Unopenable => {
                    tracing::error!(
                        table = "platform.oidc_connection",
                        "sealed secret is not openable by any held sealing key"
                    );
                    report.remaining += 1;
                }
                Reseal::Rewrapped(bytes) => {
                    if swap_platform_sealed_secret(&self.operator, &previous, &bytes).await? {
                        report.rewrapped += 1;
                    } else {
                        report.remaining += 1;
                    }
                }
            }
        }
        tracing::info!(
            write_key = %self.keyring.write_key_id(),
            rewrapped = report.rewrapped,
            current = report.current,
            remaining = report.remaining,
            "sealed provider secret rewrap finished"
        );
        Ok(report)
    }

    /// Classify one ciphertext and reseal it when needed.
    fn reseal(&self, sealed: &[u8]) -> Reseal {
        match self.keyring.rewrap(sealed) {
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
