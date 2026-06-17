//! Production mount checks for authz-check policy and audit hooks.

use crate::state::AppState;

/// Production application-state build errors.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BuildError {
    /// Stub allow policy hook was mounted in production state.
    #[error("stub PolicyHook mounted in production build")]
    StubAllowInProduction,
    /// Inbound request-id propagation was enabled without trusted upstreams.
    #[error("trusted_request_id_propagation = true requires non-empty trusted_upstreams")]
    RequestIdPropagationWithoutTrustedUpstreams,
    /// Noop authz-check audit writer was mounted in production state.
    #[error("stub AuthzAuditWriter mounted in production build")]
    NoopAuditWriterInProduction,
}

impl AppState {
    /// Verify that assembled state is safe to run as production.
    ///
    /// # Errors
    /// Returns a specific [`BuildError`] when a dev/test default is still mounted.
    pub fn build_production(self) -> Result<Self, BuildError> {
        if self.policy_hook.is_stub_default() {
            return Err(BuildError::StubAllowInProduction);
        }
        if self.trusted_request_id_propagation && self.trusted_upstreams.is_empty() {
            return Err(BuildError::RequestIdPropagationWithoutTrustedUpstreams);
        }
        if self.audit_writer.is_stub_default() {
            return Err(BuildError::NoopAuditWriterInProduction);
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use object_store::local::LocalFileSystem;
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use wyrd_auth_check::DenyAllPolicyHook;
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use super::BuildError;
    use crate::auth::audit_writer::{AuthzAuditWriter, NoopAuthzAuditWriter};
    use crate::state::AppState;

    #[tokio::test]
    async fn build_production_rejects_stub_policy_hook() {
        let result = test_state().build_production();

        assert!(matches!(result, Err(BuildError::StubAllowInProduction)));
    }

    #[tokio::test]
    async fn build_production_rejects_stub_audit_writer() {
        let mut state = test_state();
        state.policy_hook = Arc::new(DenyAllPolicyHook {
            reason: "test-deny".to_owned(),
        });
        state.audit_writer = Arc::new(NoopAuthzAuditWriter);

        let result = state.build_production();

        assert!(matches!(
            result,
            Err(BuildError::NoopAuditWriterInProduction)
        ));
    }

    #[tokio::test]
    async fn build_production_rejects_untrusted_request_id_configuration() {
        let mut state = test_state();
        state.policy_hook = Arc::new(DenyAllPolicyHook {
            reason: "test-deny".to_owned(),
        });
        state.audit_writer = Arc::new(ReadyAuditWriter);
        state.trusted_request_id_propagation = true;

        let result = state.build_production();

        assert!(matches!(
            result,
            Err(BuildError::RequestIdPropagationWithoutTrustedUpstreams)
        ));
    }

    #[derive(Debug)]
    struct ReadyAuditWriter;

    #[async_trait::async_trait]
    impl AuthzAuditWriter for ReadyAuditWriter {
        async fn write_authz_check(
            &self,
            _: &mut wyrd_sql::TenantConn<'_>,
            _: &wyrd_auth_check::AuthzCheckContext,
            _: &wyrd_spec::card::policy::PolicyDecision,
        ) -> Result<(), wyrd_spec::error::WyrdError> {
            Ok(())
        }
    }

    fn test_state() -> AppState {
        let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let root = tempfile::tempdir().expect("temp dir");
        let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
        let object_store =
            Arc::new(LocalFileSystem::new_with_prefix(root.path()).expect("local object store"));

        AppState::new(
            app_pool,
            None,
            Arc::new(StorageHandle::new(
                BackendSigner::Local(signer),
                object_store,
            )),
        )
    }
}
