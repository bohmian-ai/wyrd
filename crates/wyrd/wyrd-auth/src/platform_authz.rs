//! Platform-control-plane authorization, coupled to its canonical audit record.
//!
//! Every decision that evaluates a platform principal's permission is recorded
//! in the same transaction that makes it, allowed and denied alike. An
//! allowance hands the open transaction back so the caller performs the
//! operation in it and commits both together; a denial commits its own record
//! and refuses. A decision that cannot be recorded rolls back and refuses, so
//! an unavailable audit log can never quietly permit a privileged operation.
//!
//! The record goes to `vala.audit_staging` through the one canonical append,
//! staged under `DataTenantId::SYSTEM_OWNER` because a platform decision has no
//! owning tenant. The ordinary `AuditPublisher` drains that sentinel tenant like
//! any other, so these decisions reach retained history through the same
//! publisher and the same reader as every tenant-plane decision. There is no
//! platform audit table, publisher, or reader.

use uuid::Uuid;
use wyrd_runtime::{AuthContext, Permission};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{AuditDetail, AuditEvent, AuditOutcome};
use wyrd_spec::vala::audit_detail::AuditErrorCode;
use wyrd_sql::{OperatorPool, SqlError, TenantConn};

use crate::audit::audit_request_id;
use std::fmt::{Debug, Formatter, Result as FmtResult};

/// Operation name recorded for every platform-plane authorization decision.
///
/// One name for the whole plane, with the evaluated permission carried in the
/// row's `permission` column, so a reader filters the plane by operation and
/// the capability by permission without parsing either.
pub const PLATFORM_AUTHZ_OPERATION: &str = "platform.authz";

/// Audited resource for a decision about the deployment's OIDC connection.
///
/// There is exactly one connection per deployment, so the spelling names the
/// object rather than an identifier.
pub const PLATFORM_CONNECTION_RESOURCE: &str = "platform:oidc_connection";

/// Audited resource for a decision about the platform administrator directory
/// as a whole, rather than about one administrator in it.
pub const PLATFORM_ADMINS_RESOURCE: &str = "platform:admins";

/// Audited resource for a decision about the tenant directory as a whole,
/// rather than about one tenant in it.
pub const PLATFORM_TENANTS_RESOURCE: &str = "platform:tenants";

/// Audited resource naming one tenant that already exists.
///
/// Used wherever the tenant id is settled before the decision is recorded, so
/// the row names the tenant the operation actually acted on.
#[must_use]
pub fn tenant_resource(tenant_id: DataTenantId) -> String {
    format!("tenant:{tenant_id}")
}

/// Audited resource naming a tenant that does not exist yet.
///
/// Provisioning cannot name a tenant id truthfully: the id it proposes is
/// discarded when the directory adopts a previously failed attempt under that
/// attempt's own id. The requested slug is the one thing a fresh and a resumed
/// attempt agree on, so it is what the decision records.
#[must_use]
pub fn tenant_slug_resource(slug: &str) -> String {
    format!("tenant_slug:{slug}")
}

/// Audited resource naming one platform administrative principal.
#[must_use]
pub fn platform_principal_resource(principal_id: Uuid) -> String {
    format!("platform_principal:{principal_id}")
}

/// Audited resource naming one platform administrative credential.
#[must_use]
pub fn platform_credential_resource(credential_id: Uuid) -> String {
    format!("platform_credential:{credential_id}")
}

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
    Transaction(#[source] SqlError),
}

/// Authorizes platform-plane operations and records every decision.
///
/// Owns the operator boundary the decision, its audit row, and the operation's
/// own `platform.*` writes commit through, because all three must share one
/// transaction and therefore one connection source.
#[derive(Clone)]
pub struct PlatformAuthorization {
    /// Cross-tenant boundary the decision and its audit record commit through.
    pool: OperatorPool,
}

impl Debug for PlatformAuthorization {
    /// Prints the handle without its pool: a connection source has no
    /// inspectable state and printing it would only add noise to a trace.
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
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
    /// The returned [`TenantConn`] is an operator transaction carrying the
    /// sentinel audit scope, so `platform.*` statements run on
    /// [`TenantConn::transaction`] exactly as they would on a raw operator
    /// transaction.
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
        resource: &str,
    ) -> Result<TenantConn<'_>, PlatformAuthzError> {
        let denied_resource = required.resource.as_str().unwrap_or("any_of");
        let action = required.action.as_str().unwrap_or("any_of");

        let allowed = match context {
            AuthContext::Platform(principal) => principal.effective_permissions.contains(required),
            // A tenant grant, wildcard included, is never platform authority.
            AuthContext::Tenant(_) => false,
        };

        let mut conn = self
            .pool
            .begin_platform_audited()
            .await
            .map_err(PlatformAuthzError::Transaction)?;

        let event = Self::decision_event(context, required, request_id, resource, allowed);
        if let Err(error) = vala_sql::queries::audit_staging::append_audit(&mut conn, &event).await
        {
            // Fail closed: a decision that cannot be recorded did not happen.
            drop(conn);
            return Err(PlatformAuthzError::AuditUnavailable(error));
        }

        if !allowed {
            // The refusal is durable even though the operation never ran.
            conn.commit()
                .await
                .map_err(PlatformAuthzError::Transaction)?;
            return Err(PlatformAuthzError::Denied {
                resource: denied_resource,
                action,
            });
        }

        Ok(conn)
    }

    /// Build the canonical audit row for one platform decision.
    ///
    /// The resource names what the decision was *about*, in the caller's own
    /// exact terms: the tenant, slug, administrator, credential, or connection
    /// the operation acted on. That is not a tenancy scope: the row is staged
    /// under the sentinel tenant regardless, because platform authority is not
    /// a tenant's.
    fn decision_event(
        context: &AuthContext,
        required: &Permission,
        request_id: &str,
        resource: &str,
        allowed: bool,
    ) -> AuditEvent {
        let principal_id = context.principal_id();
        let outcome = if allowed {
            AuditOutcome::Allowed
        } else {
            AuditOutcome::Denied
        };
        AuditEvent {
            request_id: audit_request_id(request_id),
            trace_id: None,
            operation: PLATFORM_AUTHZ_OPERATION.to_owned(),
            resource: resource.to_owned(),
            card_ref: None,
            principal_id,
            principal_kind: context.principal_kind(),
            credential_id: context.credential_id(),
            permission: required.to_string(),
            outcome,
            detail: Some(AuditDetail::AuthzCheck {
                caller_principal_id: principal_id,
                callee_principal_id: principal_id,
                delegation_chain: Vec::new(),
                outcome,
                deny_reason: (!allowed).then_some(AuditErrorCode::PermissionDenied),
            }),
        }
    }
}

#[cfg(test)]
mod pg_tests {
    //! Decision and canonical-audit coupling against real Postgres.

    use super::{PLATFORM_TENANTS_RESOURCE, tenant_resource};
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

    use super::{PLATFORM_AUTHZ_OPERATION, PlatformAuthorization, PlatformAuthzError};

    /// A platform context holding exactly `permissions`, backed by a real row.
    async fn platform_context(fixture: &PgFixture, permissions: PermissionSet) -> AuthContext {
        platform_context_of_kind(fixture, PrincipalKindTag::GlobalAdmin, permissions).await
    }

    /// A platform context of a given stored kind, backed by a real row.
    ///
    /// The kind is written to `platform.principals` and carried on the context
    /// the way session verification carries it, so a test can assert what the
    /// decision records for a machine root and for a registered human.
    async fn platform_context_of_kind(
        fixture: &PgFixture,
        kind: PrincipalKindTag,
        permissions: PermissionSet,
    ) -> AuthContext {
        let id = Uuid::now_v7();
        insert_platform_principal(
            fixture.operator_pool(),
            id,
            kind,
            &format!("principal-{id}"),
        )
        .await
        .expect("platform principal inserts");
        AuthContext::from(PlatformPrincipal::new(
            PrincipalId::new(id),
            kind,
            permissions,
        ))
    }

    /// Read the `principal_kind` one staged decision recorded.
    async fn staged_principal_kind(fixture: &PgFixture, principal: PrincipalId) -> String {
        let admin = fixture.superuser_pool().await.expect("superuser pool");
        sqlx::query_scalar(
            "SELECT principal_kind FROM vala.audit_staging
              WHERE data_tenant_id = $1 AND principal_id = $2 AND operation = $3",
        )
        .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
        .bind(principal.as_uuid())
        .bind(PLATFORM_AUTHZ_OPERATION)
        .fetch_one(&admin)
        .await
        .expect("staged decision is readable")
    }

    /// Count canonical staged rows for one principal, by outcome.
    ///
    /// Reads the sentinel tenant's staging rows, which is where a platform
    /// decision stages: there is no platform audit table to read instead. The
    /// read goes through the superuser pool on purpose — the operator role is
    /// granted only `INSERT` on staging, so the production boundary cannot see
    /// what it wrote.
    async fn staged_rows(fixture: &PgFixture, principal: PrincipalId, outcome: &str) -> i64 {
        let admin = fixture.superuser_pool().await.expect("superuser pool");
        sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_staging
              WHERE data_tenant_id = $1
                AND principal_id = $2
                AND operation = $3
                AND outcome = $4",
        )
        .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
        .bind(principal.as_uuid())
        .bind(PLATFORM_AUTHZ_OPERATION)
        .bind(outcome)
        .fetch_one(&admin)
        .await
        .expect("staged count reads")
    }

    /// Read the `credential_id` of each staged decision, oldest first.
    ///
    /// Ordered by the staged sequence so the caller can compare the decisions
    /// against the order it made them in.
    async fn staged_credential_ids(
        fixture: &PgFixture,
        principal: PrincipalId,
    ) -> Vec<Option<Uuid>> {
        let admin = fixture.superuser_pool().await.expect("superuser pool");
        sqlx::query_scalar(
            "SELECT credential_id FROM vala.audit_staging
              WHERE data_tenant_id = $1 AND principal_id = $2 AND operation = $3
              ORDER BY seq",
        )
        .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
        .bind(principal.as_uuid())
        .bind(PLATFORM_AUTHZ_OPERATION)
        .fetch_all(&admin)
        .await
        .expect("staged decisions are readable")
    }

    /// An allowance is staged in the transaction the caller goes on to use, so
    /// the decision and the operation commit together.
    #[tokio::test]
    async fn an_allowance_commits_with_the_operation() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let mut permissions = PermissionSet::new();
        permissions.insert(Permission::tenant_create());
        let context = platform_context(&fixture, permissions).await;
        let principal = context.principal_id();

        let authz = PlatformAuthorization::new(fixture.operator_pool().clone());
        let conn = authz
            .authorize(
                &context,
                &Permission::tenant_create(),
                "req-allow",
                PLATFORM_TENANTS_RESOURCE,
            )
            .await
            .expect("authorized");

        assert_eq!(
            staged_rows(&fixture, principal, "allowed").await,
            0,
            "the allowance is not visible until the caller commits"
        );
        conn.commit().await.expect("caller commits");
        assert_eq!(staged_rows(&fixture, principal, "allowed").await, 1);
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
        let conn = authz
            .authorize(
                &context,
                &Permission::tenant_create(),
                "req-rollback",
                PLATFORM_TENANTS_RESOURCE,
            )
            .await
            .expect("authorized");
        drop(conn);

        assert_eq!(staged_rows(&fixture, principal, "allowed").await, 0);
    }

    /// The decision records the principal's stored kind, not the plane's.
    ///
    /// Both platform kinds hold the same fixed grant and perform the same
    /// operation, so the audit row's `principal_kind` is the only thing that
    /// separates a machine root's decision from a registered human's. It used to
    /// be hard-coded, which attributed every human decision to a machine.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start or either kind is misrecorded.
    #[tokio::test]
    async fn a_decision_records_the_stored_principal_kind() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let authz = PlatformAuthorization::new(fixture.operator_pool().clone());

        for (kind, expected) in [
            (PrincipalKindTag::GlobalAdmin, "global_admin"),
            (PrincipalKindTag::User, "user"),
        ] {
            let mut permissions = PermissionSet::new();
            permissions.insert(Permission::tenant_create());
            let context = platform_context_of_kind(&fixture, kind, permissions).await;
            let principal = context.principal_id();

            let conn = authz
                .authorize(
                    &context,
                    &Permission::tenant_create(),
                    "req-kind",
                    PLATFORM_TENANTS_RESOURCE,
                )
                .await
                .expect("authorized");
            conn.commit().await.expect("allowance commits");

            assert_eq!(
                staged_principal_kind(&fixture, principal).await,
                expected,
                "a {expected} decision is recorded as some other kind"
            );
        }
    }

    /// The decision names the credential its session was minted from.
    ///
    /// One principal holds two credentials at once during a rotation, so the
    /// principal id alone cannot say which key made a given decision — which is
    /// exactly what an operator retiring a leaked credential needs to know. A
    /// federated session presents no credential, so its decisions name none.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start or a decision names the wrong
    /// credential.
    #[tokio::test]
    async fn a_decision_records_the_credential_it_was_made_with() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let authz = PlatformAuthorization::new(fixture.operator_pool().clone());
        let mut permissions = PermissionSet::new();
        permissions.insert(Permission::tenant_create());
        let context =
            platform_context_of_kind(&fixture, PrincipalKindTag::GlobalAdmin, permissions).await;
        let principal = context.principal_id();
        let AuthContext::Platform(base) = context else {
            panic!("a platform context is not a tenant context");
        };

        let original = Uuid::now_v7();
        let replacement = Uuid::now_v7();
        for credential in [Some(original), Some(replacement), None] {
            let conn = authz
                .authorize(
                    &AuthContext::from(base.clone().with_credential_id(credential)),
                    &Permission::tenant_create(),
                    "req-credential",
                    PLATFORM_TENANTS_RESOURCE,
                )
                .await
                .expect("authorized");
            conn.commit().await.expect("allowance commits");
        }

        assert_eq!(
            staged_credential_ids(&fixture, principal).await,
            vec![Some(original), Some(replacement), None],
            "the three decisions did not each name the credential they were made with"
        );
    }

    /// A denial is durable even though the operation never ran, and names the
    /// principal, permission, and the tenant the decision was about.
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
                &tenant_resource(target),
            )
            .await
            .err()
            .expect("denied");

        assert!(matches!(error, PlatformAuthzError::Denied { .. }));
        assert_eq!(staged_rows(&fixture, principal, "denied").await, 1);

        let row = sqlx::query(
            "SELECT resource, permission FROM vala.audit_staging
              WHERE data_tenant_id = $1 AND principal_id = $2",
        )
        .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
        .bind(principal.as_uuid())
        .fetch_one(&fixture.superuser_pool().await.expect("superuser pool"))
        .await
        .expect("denial row reads");
        assert_eq!(row.get::<String, _>("resource"), format!("tenant:{target}"));
        assert_eq!(
            row.get::<String, _>("permission"),
            Permission::tenant_suspend().to_string()
        );
    }

    /// A tenant-scoped context never authorizes on the platform plane, and the
    /// refusal is staged like any other denial.
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
                PLATFORM_TENANTS_RESOURCE,
            )
            .await
            .err()
            .expect("refused");

        assert!(matches!(error, PlatformAuthzError::Denied { .. }));
        assert_eq!(
            staged_rows(&fixture, principal, "denied").await,
            1,
            "a wildcard tenant grant never becomes platform authority"
        );
    }

    /// When the canonical audit log cannot accept the decision, the operation is
    /// refused and nothing is committed.
    #[tokio::test]
    async fn an_unrecordable_decision_fails_closed() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let mut permissions = PermissionSet::new();
        permissions.insert(Permission::tenant_create());
        let context = platform_context(&fixture, permissions).await;
        let principal = context.principal_id();

        // Remove the staging table's insert privilege for the operator role so
        // the append fails exactly as an unavailable audit log would.
        let admin = fixture.superuser_pool().await.expect("superuser pool");
        sqlx::query("REVOKE INSERT ON vala.audit_staging FROM wyrd_platform_admin")
            .execute(&admin)
            .await
            .expect("privilege revoked");

        let authz = PlatformAuthorization::new(fixture.operator_pool().clone());
        let error = authz
            .authorize(
                &context,
                &Permission::tenant_create(),
                "req-no-audit",
                PLATFORM_TENANTS_RESOURCE,
            )
            .await
            .err()
            .expect("refused when the decision cannot be recorded");

        sqlx::query("GRANT INSERT ON vala.audit_staging TO wyrd_platform_admin")
            .execute(&admin)
            .await
            .expect("privilege restored");

        assert!(matches!(error, PlatformAuthzError::AuditUnavailable(_)));
        assert_eq!(staged_rows(&fixture, principal, "allowed").await, 0);
    }
}
