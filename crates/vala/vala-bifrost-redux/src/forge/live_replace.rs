//! Transactional SQL lineage settlement for live Iceberg replacements.

#[cfg(feature = "test-support")]
use std::sync::atomic::{AtomicBool, Ordering};

use vala_sql::queries::forge_operations::ForgeOperations;
use vala_sql::row_types::forge_operations::{ForgeOperationFamily, ForgeOperationTransition};
use wyrd_spec::vala::api::AuditDetail;

use super::Forge;
use super::compact::ForgeGroupKey;
use super::error::ForgeError;
use super::lease::ForgeLease;

#[cfg(feature = "test-support")]
/// Injects one pre-`Prepared` transition failure for the real catalog integration seam.
static FAIL_NEXT_PREPARED_LIVE_TRANSITION: AtomicBool = AtomicBool::new(false);

#[cfg(feature = "test-support")]
/// Injects one terminal live-rewrite failure before its transaction becomes durable.
static FAIL_NEXT_TERMINAL_LIVE_TRANSITION: AtomicBool = AtomicBool::new(false);

impl Forge {
    /// Fail the next test-support `Prepared` live-replacement transition.
    ///
    /// This is a single-use integration seam. It fails before any lineage row
    /// is durable so callers can prove that verified outputs remain retained
    /// for the fenced orphan-GC lifecycle while the error stays authoritative.
    #[cfg(feature = "test-support")]
    pub fn fail_next_prepared_live_transition_for_test(&self) {
        FAIL_NEXT_PREPARED_LIVE_TRANSITION.store(true, Ordering::Release);
    }

    /// Fail the next test-support terminal live-replacement transition.
    ///
    /// This single-use integration seam rejects the transaction before the
    /// terminal operation-state transition becomes durable.
    #[cfg(feature = "test-support")]
    pub fn fail_next_terminal_live_transition_for_test(&self) {
        FAIL_NEXT_TERMINAL_LIVE_TRANSITION.store(true, Ordering::Release);
    }

    /// Record one live-replacement operation-state transition atomically.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError`] when lease renewal, tenant transaction creation,
    /// the operation-state transition, fence assertion, or transaction commit
    /// fails. The caller-owned transaction rolls back the durable row.
    pub(super) async fn append_live_transition(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeGroupKey,
        operation: &str,
        detail: AuditDetail,
    ) -> Result<(), ForgeError> {
        if !lease.renew(&self.core.operator_pool).await? {
            return Err(ForgeError::FenceLost {
                lease_key: lease.lease_key.clone(),
            });
        }
        let mut conn = self
            .core
            .vala
            .tenant_conn(key.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        lease.require_fence(&self.core.operator_pool).await?;
        #[cfg(feature = "test-support")]
        if operation == "forge.iceberg_rewrite.prepared"
            && FAIL_NEXT_PREPARED_LIVE_TRANSITION.swap(false, Ordering::AcqRel)
        {
            return Err(ForgeError::Invariant {
                detail: "injected Prepared live-replacement transition failure".to_owned(),
            });
        }
        #[cfg(feature = "test-support")]
        if operation != "forge.iceberg_rewrite.prepared"
            && FAIL_NEXT_TERMINAL_LIVE_TRANSITION.swap(false, Ordering::AcqRel)
        {
            return Err(ForgeError::Invariant {
                detail: "injected live terminal transition failure".to_owned(),
            });
        }
        let resource = key.audit_resource();
        let operations = ForgeOperations::new(&resource, ForgeOperationFamily::IcebergRewrite)
            .map_err(ForgeError::Sql)?;
        let transition = if operation == "forge.iceberg_rewrite.prepared" {
            operations
                .append_prepared(&mut conn, operation, &detail)
                .await
        } else {
            operations
                .append_terminal(&mut conn, operation, &detail)
                .await
        }
        .map_err(ForgeError::Sql)?;
        match transition {
            ForgeOperationTransition::Applied | ForgeOperationTransition::AlreadyApplied => {}
        }
        lease.assert_transaction_fence(&mut conn).await?;
        conn.commit().await.map_err(ForgeError::Sql)
    }
}
