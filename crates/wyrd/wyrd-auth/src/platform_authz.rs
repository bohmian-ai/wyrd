//! Platform-control-plane authorization, coupled to its audit record.
//!
//! Every decision that evaluates a platform principal's permission is recorded
//! in the same transaction that makes it, allowed and denied alike. An
//! allowance hands the open transaction back so the caller performs the
//! operation in it and commits both together; a denial commits its own record
//! and refuses. A decision that cannot be recorded rolls back and refuses, so
//! an unavailable audit log can never quietly permit a privileged operation.

use sqlx::{Postgres, Transaction};
use uuid::Uuid;
use wyrd_runtime::{AuthContext, Permission};
use wyrd_spec::DataTenantId;
use wyrd_sql::queries::platform::audit_authz::{PlatformAuthzAudit, insert_platform_authz_audit};
use wyrd_sql::{OperatorPool, SqlError};

/// Stable denial reason recorded when a principal lacks the permission.
const REASON_PERMISSION_NOT_HELD: &str = "permission_not_held";

/// Stable denial reason recorded when a tenant-scope context reaches the plane.
const REASON_WRONG_PLANE: &str = "wrong_control_plane";

/// Platform authorization failure.
#[derive(Debug, thiserror::Error)]
pub enum PlatformAuthzError {
    /// The principal is not authorized. The denial is recorded before this is
    /// returned.
    #[error("platform principal is not authorized for {resource}:{action}")]
    Denied {
        /// Resource label the decision named.
        resource: &'static str,
        /// Action label the decision named.
        action: &'static str,
    },
    /// The decision could not be recorded, so the operation must not proceed.
    #[error("platform authorization could not be audited: {0}")]
    AuditUnavailable(#[source] SqlError),
    /// The transaction could not be opened or committed.
    #[error("platform authorization transaction failed: {0}")]
    Transaction(#[source] sqlx::Error),
}

/// Authorizes platform-plane operations and records every decision.
///
/// Owns the operator boundary the decision and its audit row commit through,
/// because the two must share one transaction and therefore one connection
/// source.
#[derive(Clone)]
pub struct PlatformAuthorization {
    /// Cross-tenant boundary the decision and its audit record commit through.
    pool: OperatorPool,
}

impl std::fmt::Debug for PlatformAuthorization {
    /// Prints the handle without its pool: a connection source has no
    /// inspectable state and printing it would only add noise to a trace.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlatformAuthorization")
            .finish_non_exhaustive()
    }
}

impl PlatformAuthorization {
    /// Bind platform authorization to one operator boundary.
    #[must_use]
    pub const fn new(pool: OperatorPool) -> Self {
        Self { pool }
    }

    /// Decide one platform-plane permission and record the decision.
    ///
    /// On success the returned transaction already carries the allowance row,
    /// so the caller's operation and its authorization commit or roll back
    /// together. The caller owns that transaction and must commit it; this
    /// function neither commits nor rolls back an allowance.
    ///
    /// A tenant-scoped context reaching this plane is a denial rather than a
    /// type error only because `AuthContext` is shared; the extractor that
    /// produces a platform caller cannot construct one, so this arm exists to
    /// keep the decision total rather than to be reachable in practice.
    ///
    /// # Errors
    /// Returns [`PlatformAuthzError::Denied`] when the grant does not cover
    /// `required`, after committing the denial record.
    /// Returns [`PlatformAuthzError::AuditUnavailable`] when the record cannot
    /// be appended; the transaction is rolled back and the caller must refuse.
    /// Returns [`PlatformAuthzError::Transaction`] when the transaction cannot
    /// be opened or the denial record cannot be committed.
    #[tracing::instrument(level = "debug", skip(self, context), err)]
    pub async fn authorize(
        &self,
        context: &AuthContext,
        required: &Permission,
        request_id: &str,
        credential_id: Option<Uuid>,
        target_tenant_id: Option<DataTenantId>,
    ) -> Result<Transaction<'_, Postgres>, PlatformAuthzError> {
        let resource = required.resource.as_str().unwrap_or("any_of");
        let action = required.action.as_str().unwrap_or("any_of");

        let deny_reason = match context {
            AuthContext::Platform(principal) => {
                if principal.effective_permissions.contains(required) {
                    None
                } else {
                    Some(REASON_PERMISSION_NOT_HELD)
                }
            }
            AuthContext::Tenant(_) => Some(REASON_WRONG_PLANE),
        };

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(PlatformAuthzError::Transaction)?;
        let appended = insert_platform_authz_audit(
            &mut tx,
            PlatformAuthzAudit {
                request_id,
                principal_id: context.principal_id().as_uuid(),
                credential_id,
                resource,
                action,
                target_tenant_id,
                deny_reason,
            },
        )
        .await;

        if let Err(error) = appended {
            // Fail closed: a decision that cannot be recorded did not happen.
            let _ = tx.rollback().await;
            return Err(PlatformAuthzError::AuditUnavailable(error));
        }

        if deny_reason.is_some() {
            // The refusal is durable even though the operation never ran.
            tx.commit().await.map_err(PlatformAuthzError::Transaction)?;
            return Err(PlatformAuthzError::Denied { resource, action });
        }

        Ok(tx)
    }
}

#[cfg(test)]
mod pg_tests {
    //! Decision and audit coupling against real Postgres.

    use sqlx::Row;
    use uuid::Uuid;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::{
        AuthContext, Permission, PermissionSet, PlatformPrincipal, Principal, PrincipalId,
        PrincipalKind,
    };
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalKindTag;
    use wyrd_sql::queries::platform::principals::insert_platform_principal;

    use super::{PlatformAuthorization, PlatformAuthzError};

    /// A platform context holding exactly `permissions`, backed by a real row.
    async fn platform_context(fixture: &PgFixture, permissions: PermissionSet) -> AuthContext {
        let id = Uuid::now_v7();
        insert_platform_principal(
            fixture.operator_pool(),
            id,
            PrincipalKindTag::GlobalAdmin,
            &format!("principal-{id}"),
        )
        .await
        .expect("platform principal inserts");
        AuthContext::from(PlatformPrincipal::new(PrincipalId::new(id), permissions))
    }

    /// Count audit rows for one principal, by decision.
    async fn audit_rows(fixture: &PgFixture, principal: PrincipalId, decision: &str) -> i64 {
        sqlx::query_scalar(
            "SELECT count(*) FROM platform.audit_authz
              WHERE principal_id = $1 AND decision = $2",
        )
        .bind(principal.as_uuid())
        .bind(decision)
        .fetch_one(fixture.operator_pool().pool())
        .await
        .expect("audit count reads")
    }

    /// An allowance is recorded in the transaction the caller goes on to use, so
    /// the decision and the operation commit together.
    #[tokio::test]
    async fn an_allowance_commits_with_the_operation() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let mut permissions = PermissionSet::new();
        permissions.insert(Permission::tenant_create());
        let context = platform_context(&fixture, permissions).await;
        let principal = context.principal_id();

        let authz = PlatformAuthorization::new(fixture.operator_pool().clone());
        let tx = authz
            .authorize(
                &context,
                &Permission::tenant_create(),
                "req-allow",
                None,
                None,
            )
            .await
            .expect("authorized");

        assert_eq!(
            audit_rows(&fixture, principal, "allow").await,
            0,
            "the allowance is not visible until the caller commits"
        );
        tx.commit().await.expect("caller commits");
        assert_eq!(audit_rows(&fixture, principal, "allow").await, 1);
    }

    /// Abandoning the operation abandons its authorization record too: an
    /// operation that never happened leaves no allowance behind.
    #[tokio::test]
    async fn a_rolled_back_operation_leaves_no_allowance() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let mut permissions = PermissionSet::new();
        permissions.insert(Permission::tenant_create());
        let context = platform_context(&fixture, permissions).await;
        let principal = context.principal_id();

        let authz = PlatformAuthorization::new(fixture.operator_pool().clone());
        let tx = authz
            .authorize(
                &context,
                &Permission::tenant_create(),
                "req-rollback",
                None,
                None,
            )
            .await
            .expect("authorized");
        tx.rollback().await.expect("caller rolls back");

        assert_eq!(audit_rows(&fixture, principal, "allow").await, 0);
    }

    /// A denial is durable even though the operation never ran, and names the
    /// principal, permission, and target tenant.
    #[tokio::test]
    async fn a_denial_is_recorded_and_refuses() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let context = platform_context(&fixture, PermissionSet::new()).await;
        let principal = context.principal_id();
        let target = DataTenantId::new_v7();

        let authz = PlatformAuthorization::new(fixture.operator_pool().clone());
        let error = authz
            .authorize(
                &context,
                &Permission::tenant_suspend(),
                "req-deny",
                None,
                Some(target),
            )
            .await
            .expect_err("denied");

        assert!(matches!(error, PlatformAuthzError::Denied { .. }));
        assert_eq!(audit_rows(&fixture, principal, "deny").await, 1);

        let row = sqlx::query(
            "SELECT resource, action, deny_reason, target_tenant_id
               FROM platform.audit_authz WHERE principal_id = $1",
        )
        .bind(principal.as_uuid())
        .fetch_one(fixture.operator_pool().pool())
        .await
        .expect("denial row reads");
        assert_eq!(row.get::<String, _>("resource"), "tenants");
        assert_eq!(row.get::<String, _>("action"), "suspend");
        assert_eq!(
            row.get::<Option<String>, _>("deny_reason").as_deref(),
            Some("permission_not_held")
        );
        assert_eq!(
            row.get::<Option<Uuid>, _>("target_tenant_id"),
            Some(target.as_uuid())
        );
    }

    /// A tenant-scoped context never authorizes on the platform plane, and the
    /// refusal is recorded as a distinct reason.
    #[tokio::test]
    async fn a_tenant_context_is_refused_on_the_platform_plane() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let mut permissions = PermissionSet::new();
        permissions.insert(Permission::wildcard());
        let tenant = Principal::new(
            PrincipalId::new(Uuid::now_v7()),
            PrincipalKind::TenantAdmin,
            fixture.data_tenant_id(),
            Vec::new(),
            permissions,
        );
        let principal = tenant.id;
        let context = AuthContext::from(tenant);

        let authz = PlatformAuthorization::new(fixture.operator_pool().clone());
        let error = authz
            .authorize(
                &context,
                &Permission::tenant_create(),
                "req-wrong-plane",
                None,
                None,
            )
            .await
            .expect_err("refused");

        assert!(matches!(error, PlatformAuthzError::Denied { .. }));
        let reason: Option<String> = sqlx::query_scalar(
            "SELECT deny_reason FROM platform.audit_authz WHERE principal_id = $1",
        )
        .bind(principal.as_uuid())
        .fetch_one(fixture.operator_pool().pool())
        .await
        .expect("denial row reads");
        assert_eq!(
            reason.as_deref(),
            Some("wrong_control_plane"),
            "a wildcard tenant grant never becomes platform authority"
        );
    }

    /// When the audit log cannot accept the decision, the operation is refused
    /// and nothing is committed.
    #[tokio::test]
    async fn an_unrecordable_decision_fails_closed() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let mut permissions = PermissionSet::new();
        permissions.insert(Permission::tenant_create());
        let context = platform_context(&fixture, permissions).await;

        // Remove the audit table's insert privilege for the operator role so the
        // append fails exactly as an unavailable audit log would.
        let admin = fixture.superuser_pool().await.expect("superuser pool");
        sqlx::query("REVOKE INSERT ON platform.audit_authz FROM wyrd_platform_admin")
            .execute(&admin)
            .await
            .expect("privilege revoked");

        let authz = PlatformAuthorization::new(fixture.operator_pool().clone());
        let error = authz
            .authorize(
                &context,
                &Permission::tenant_create(),
                "req-no-audit",
                None,
                None,
            )
            .await
            .expect_err("refused when the decision cannot be recorded");

        assert!(matches!(error, PlatformAuthzError::AuditUnavailable(_)));

        sqlx::query("GRANT INSERT ON platform.audit_authz TO wyrd_platform_admin")
            .execute(&admin)
            .await
            .expect("privilege restored");
    }
}
