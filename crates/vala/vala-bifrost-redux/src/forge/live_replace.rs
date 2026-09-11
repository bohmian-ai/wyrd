//! Transactional SQL and audit settlement for live Iceberg replacements.

#[cfg(feature = "test-support")]
use std::sync::atomic::{AtomicBool, Ordering};

use vala_sql::queries::forge_operations::ForgeOperations;
use vala_sql::row_types::forge_operations::{ForgeOperationFamily, ForgeOperationTransition};
use wyrd_spec::vala::api::AuditDetail;

use super::Forge;
use super::compact::{ForgeGroupKey, forge_transition_event};
use super::error::ForgeError;
use super::lease::ForgeLease;

#[cfg(feature = "test-support")]
/// Injects one pre-`Prepared` audit failure for the real catalog integration seam.
static FAIL_NEXT_PREPARED_LIVE_AUDIT: AtomicBool = AtomicBool::new(false);

#[cfg(feature = "test-support")]
/// Injects one terminal live-rewrite audit failure before its transaction becomes durable.
static FAIL_NEXT_TERMINAL_LIVE_AUDIT: AtomicBool = AtomicBool::new(false);

impl Forge {
    /// Fail the next test-support `Prepared` live-replacement audit append.
    ///
    /// This is a single-use integration seam. It fails before any audit row is
    /// durable so callers can prove that verified outputs remain retained for
    /// the fenced orphan-GC lifecycle while the audit error stays authoritative.
    #[cfg(feature = "test-support")]
    pub fn fail_next_prepared_live_audit_for_test(&self) {
        FAIL_NEXT_PREPARED_LIVE_AUDIT.store(true, Ordering::Release);
    }

    /// Fail the next test-support terminal live-replacement audit append.
    ///
    /// This single-use integration seam rejects the transaction before either
    /// the terminal audit or operation-state transition becomes durable.
    #[cfg(feature = "test-support")]
    pub fn fail_next_terminal_live_audit_for_test(&self) {
        FAIL_NEXT_TERMINAL_LIVE_AUDIT.store(true, Ordering::Release);
    }

    /// Append one live-replacement audit and projection transition atomically.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError`] when lease renewal, tenant transaction creation,
    /// operation-state transition, audit append, fence assertion, or transaction
    /// commit fails. The caller-owned transaction rolls back both durable rows.
    pub(super) async fn append_live_audit(
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
            && FAIL_NEXT_PREPARED_LIVE_AUDIT.swap(false, Ordering::AcqRel)
        {
            return Err(ForgeError::Invariant {
                detail: "injected Prepared audit append failure".to_owned(),
            });
        }
        #[cfg(feature = "test-support")]
        if operation != "forge.iceberg_rewrite.prepared"
            && FAIL_NEXT_TERMINAL_LIVE_AUDIT.swap(false, Ordering::AcqRel)
        {
            return Err(ForgeError::Invariant {
                detail: "injected live terminal audit append failure".to_owned(),
            });
        }
        let resource = key.audit_resource();
        let event = forge_transition_event(operation, resource.clone(), detail);
        let operations = ForgeOperations::new(&resource, ForgeOperationFamily::IcebergRewrite)
            .map_err(ForgeError::Sql)?;
        let transition = if operation == "forge.iceberg_rewrite.prepared" {
            operations.append_prepared(&mut conn, &event).await
        } else {
            operations.append_terminal(&mut conn, &event).await
        }
        .map_err(ForgeError::Sql)?;
        match transition {
            ForgeOperationTransition::Applied { .. }
            | ForgeOperationTransition::AlreadyApplied { .. } => {}
        }
        lease.assert_transaction_fence(&mut conn).await?;
        conn.commit().await.map_err(ForgeError::Sql)
    }
}
