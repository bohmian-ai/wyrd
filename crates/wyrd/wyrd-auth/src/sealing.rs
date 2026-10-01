//! Sealing-key rotation: rewrap every long-lived sealed secret under the write key.
//!
//! The pass covers tenant human-connection and workload-issuer client secrets,
//! the platform connection secret, and every stored sealed column (access
//! token, refresh token or bootstrap API key, CSRF token) of production UI
//! browser sessions — including sessions past their absolute expiry whose rows
//! have not yet been purged — so `remaining == 0` speaks for every ciphertext
//! the provider and browser-session stores hold.
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
//! Completed human logins (`wyrd.auth_login_state.completion_sealed`) are
//! deliberately not rewrapped: each is redeemable for at most the login
//! completion TTL (two minutes) after it is sealed and is unreadable after.
//! A completion sealed under K1 therefore needs K1 only until that TTL has
//! passed since the last K1 writer stopped. Retaining K1 for that long past
//! the verification pass's start is enough; rewrapping it would only extend
//! a secret that is about to expire.
//!
//! The pass is idempotent and safe across replicas: each column value is
//! replaced by a compare-and-swap on the exact bytes read, so a concurrent
//! reconfiguration, browser-session renewal or logout, or a second replica's
//! rewrap is never overwritten; the value it lost to counts as `remaining`
//! and the next pass reseals it. It is an engine-internal
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

    /// Reseal every tenant, platform, and browser-session secret not under the
    /// write key.
    ///
    /// Walks human connections, workload trusted issuers, each sealed column of
    /// every stored browser session (expired but unpurged rows included), and
    /// the platform connection. A ciphertext no held
    /// key opens, or one whose swap loses to a concurrent write, is counted in
    /// `remaining`; the former is logged with its table and tenant, never its
    /// bytes.
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
            "sealed secret rewrap finished"
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
        // Without a keyring nothing opens; `rewrap` answers `None` for a
        // ciphertext already under the write key.
        match self.keyring.as_ref().map(|keyring| keyring.rewrap(sealed)) {
            Some(Ok(None)) => report.current += 1,
            Some(Ok(Some(bytes))) => {
                if swap(bytes).await? {
                    report.rewrapped += 1;
                } else {
                    report.remaining += 1;
                }
            }
            None | Some(Err(_)) => {
                tracing::error!(
                    table,
                    tenant_id = ?tenant_id,
                    "sealed secret is not openable by any held sealing key"
                );
                report.remaining += 1;
            }
        }
        Ok(())
    }
}
