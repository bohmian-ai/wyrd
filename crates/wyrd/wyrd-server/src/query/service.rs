//! Auth-bound adapters from Gate transports and typed plans into retained Oracle.

use vala_bifrost_redux::oracle::{AuthorizedQueryContext, OracleQueryStream, QueryIpcDecodeError};
use wyrd_runtime::{Action, Permission, PermissionDenyReason, PermissionVerdict, Resource};
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditOutcome, AuthMethod, BifrostQueryRequest, CancelRunningQueryResponse, RunningQuerySummary,
};

use crate::AppState;
use crate::audit;
use crate::components::auth::Caller;
use crate::http::error::permission_deny_reason_to_wyrd;

/// Authorization and audit owner for one authenticated query-plane operation.
///
/// Query authorization has two distinct stages and both belong here. Route
/// admission is deliberately coarse — a schema- or table-scoped grant cannot
/// satisfy an object-wide requirement, so an exact-global prerequisite would
/// reject every scoped principal before Oracle resolved which tables the SQL
/// touches. The authoritative object decision is taken inside Oracle against
/// the resolved scan set, and this owner records its denial. Lifecycle
/// operations, which name their target by request ID rather than by object,
/// take the exact decision here directly.
///
/// Every refusal on either stage fails closed on its audit append: a denial
/// that cannot be recorded is returned as audit-unavailable rather than as a
/// plain rejection, so no refusal is silently unlogged.
pub(crate) struct QueryAuthority<'a> {
    /// Shared server state containing authorization and the durable audit outbox.
    state: &'a AppState,
    /// Authenticated caller whose tenant and request spine own the audit row.
    caller: &'a Caller,
    /// Stable operation name for the public action being decided.
    operation: &'static str,
    /// Redacted resource, optionally qualified by target request ID.
    resource: String,
    /// Existing permission required by every public query-plane action.
    permission: Permission,
}

impl<'a> QueryAuthority<'a> {
    /// Binds one operation and redacted resource to the query-read permission.
    pub(crate) fn new(
        state: &'a AppState,
        caller: &'a Caller,
        operation: &'static str,
        resource: impl Into<String>,
    ) -> Self {
        Self {
            state,
            caller,
            operation,
            resource: resource.into(),
            permission: Permission::bifrost_query_read(),
        }
    }

    /// Binds one lifecycle operation to its authenticated caller and redacted target.
    fn lifecycle(
        state: &'a AppState,
        caller: &'a Caller,
        operation: &'static str,
        request_id: Option<&RequestId>,
    ) -> Self {
        let resource = request_id.map_or_else(
            || "vala.query.lifecycle".to_owned(),
            |request_id| format!("vala.query.lifecycle/{request_id}"),
        );
        Self::new(state, caller, operation, resource)
    }

    /// Admits a request that holds the Bifrost read capability under some scope.
    ///
    /// This is admission, never authorization: it proves only that the caller
    /// operates the Bifrost read surface at all. A principal admitted here is
    /// still refused every table its grants do not name.
    ///
    /// # Errors
    ///
    /// Returns the stable authorization denial, or audit-unavailable when that
    /// denial cannot be durably recorded.
    async fn admit_capability(&self) -> Result<(), WyrdError> {
        if self
            .caller
            .principal
            .effective_permissions
            .covers_operation(&Resource::BifrostQuery, &Action::Read)
        {
            return Ok(());
        }
        Err(self
            .deny(PermissionDenyReason::Rbac {
                required: Box::new(self.permission.clone()),
                principal: self.caller.principal.id,
            })
            .await)
    }

    /// Authorizes one exact permission and durably audits a denial.
    ///
    /// Successful authorization deliberately writes no read-decision row here:
    /// retained Oracle commits the immutable visibility decision after planning
    /// and before any query data is read.
    ///
    /// # Errors
    ///
    /// Returns the stable authorization denial or audit-unavailable error.
    pub(crate) async fn authorize(&self) -> Result<(), WyrdError> {
        match self
            .state
            .authz
            .permission_check
            .check(&self.caller.principal, &self.permission)
        {
            PermissionVerdict::Allow => Ok(()),
            PermissionVerdict::Deny { reason } => Err(self.deny(reason).await),
        }
    }

    /// Durably records one authorization denial and returns what the caller sees.
    ///
    /// A failed append substitutes audit-unavailable for the rejection, which is
    /// the fail-closed outcome: the caller is still refused, and the refusal is
    /// never reported as if it had been recorded.
    async fn deny(&self, reason: PermissionDenyReason) -> WyrdError {
        match self.append(AuditOutcome::Denied).await {
            Ok(()) => permission_deny_reason_to_wyrd(reason),
            Err(error) => error,
        }
    }

    /// Durably records the authoritative object denial Oracle already decided.
    ///
    /// The row carries only the caller, the operation, and the operation-only
    /// permission token. The refused table never enters the audit payload: the
    /// object identity stays inside Oracle, exactly as the raw SQL does.
    ///
    /// Returns audit-unavailable when the append fails, and otherwise the
    /// caller's original stable query-forbidden error unchanged.
    async fn record_object_denial(&self, denial: WyrdError) -> WyrdError {
        #[cfg(feature = "test-support")]
        if self
            .state
            .query_control_audit_fault
            .as_ref()
            .is_some_and(crate::state::QueryControlAuditFaultController::object_denials_fail)
        {
            return wyrd_spec::vala::error::BifrostError::AuditUnavailable {
                detail: "object denial audit unavailable".to_owned(),
            }
            .into();
        }
        match self.append(AuditOutcome::Denied).await {
            Ok(()) => denial,
            Err(error) => error,
        }
    }

    /// Appends one decision row for this operation to the tenant audit staging.
    ///
    /// # Errors
    ///
    /// Returns audit-unavailable when the tenant append cannot commit.
    async fn append(&self, outcome: AuditOutcome) -> Result<(), WyrdError> {
        let event = audit::audit_event(
            self.caller,
            self.operation,
            &self.resource,
            &self.permission.to_string(),
            outcome,
        );
        audit::record_audit_owned(
            self.state.postgres.vala_pool().clone(),
            self.caller.data_tenant_id,
            event,
        )
        .await
    }

    /// Durably records the allowed decision before the owner is dispatched.
    ///
    /// A lifecycle operation has no later Oracle read decision to carry its
    /// `allowed` row, so the boundary writes it here, before dispatch. The row
    /// states only that the caller was permitted; whether the owner then
    /// succeeded is lifecycle state, not an authorization decision.
    ///
    /// # Errors
    ///
    /// Returns audit-unavailable before the lifecycle owner is called.
    async fn record_admission(&self) -> Result<(), WyrdError> {
        #[cfg(feature = "test-support")]
        if self
            .state
            .query_control_audit_fault
            .as_ref()
            .is_some_and(crate::state::QueryControlAuditFaultController::cancel_attempts_fail)
        {
            return Err(wyrd_spec::vala::error::BifrostError::AuditUnavailable {
                detail: "lifecycle audit gate unavailable before dispatch".to_owned(),
            }
            .into());
        }
        let event = audit::audit_event(
            self.caller,
            self.operation,
            &self.resource,
            &self.permission.to_string(),
            AuditOutcome::Allowed,
        );
        audit::record_audit_owned(
            self.state.postgres.vala_pool().clone(),
            self.caller.data_tenant_id,
            event,
        )
        .await
    }
}

/// Converts the authenticated server caller into Oracle's exact context.
///
/// The verified delegation chain travels with the effective principal so both
/// local and forwarded execution build the same attributed read decision.
///
/// # Errors
///
/// Returns the tenant invariant error if the principal and extractor tenant
/// disagree.
fn oracle_context(caller: &Caller) -> Result<AuthorizedQueryContext, WyrdError> {
    AuthorizedQueryContext::try_new(
        caller.principal.clone(),
        caller.data_tenant_id,
        caller.request_id.clone(),
        None,
        AuthMethod::Jwt,
        Permission::bifrost_query_read(),
    )
    .map(|context| context.with_delegation_chain(caller.delegation_chain.clone()))
    .map_err(Into::into)
}

/// Authorizes and dispatches a public SQL request through the stable local Gate.
///
/// Authorization always completes before the Gate inspects local Oracle
/// availability. The returned stream owns admission, cancellation, and cleanup
/// guards, so dropping it on transport cancellation stops query work.
///
/// # Errors
///
/// Returns authorization/audit errors, Oracle role unavailable, or a stable
/// pre-stream Oracle query error.
pub async fn stream_query(
    state: AppState,
    caller: Caller,
    request: BifrostQueryRequest,
) -> Result<OracleQueryStream, WyrdError> {
    let authority = QueryAuthority::new(&state, &caller, "vala.query.sync", "vala.query");
    authority.admit_capability().await?;
    let context = oracle_context(&caller)?;
    match state.bifrost.query_sql(context, request).await {
        Ok(stream) => Ok(stream),
        // Oracle took the authoritative object decision against its resolved
        // scan set. The tenant and principal are verified here, so the refusal
        // is recorded on the same audited boundary as a coarse route denial.
        Err(denial @ wyrd_spec::vala::error::BifrostError::QueryForbidden) => {
            Err(authority.record_object_denial(denial.into()).await)
        }
        Err(other) => Err(other.into()),
    }
}

/// Lists active queries visible to the authenticated tenant.
///
/// Cancelling this future abandons the pending distributed lookup; no query
/// lifecycle is changed.
///
/// # Errors
///
/// Returns stable authorization, audit, role-availability, or owner-control errors.
pub async fn list_running_queries(
    state: &AppState,
    caller: &Caller,
) -> Result<Vec<RunningQuerySummary>, WyrdError> {
    let audit = QueryAuthority::lifecycle(state, caller, "vala.query.running.list", None);
    audit.authorize().await?;
    audit.record_admission().await?;
    match state.bifrost.query_controls() {
        Some(controls) => controls.list(caller.data_tenant_id).await,
        None => Err(wyrd_spec::vala::error::BifrostError::OracleRoleUnavailable.into()),
    }
}

/// Returns one active query visible to the authenticated tenant.
///
/// Cancelling this future abandons the pending distributed lookup; no query
/// lifecycle is changed.
///
/// # Errors
///
/// Returns stable authorization, audit, role-availability, not-found, or conflict errors.
pub async fn get_running_query(
    state: &AppState,
    caller: &Caller,
    request_id: RequestId,
) -> Result<RunningQuerySummary, WyrdError> {
    let audit =
        QueryAuthority::lifecycle(state, caller, "vala.query.running.get", Some(&request_id));
    audit.authorize().await?;
    audit.record_admission().await?;
    match state.bifrost.query_controls() {
        Some(controls) => controls.get(caller.data_tenant_id, request_id).await,
        None => Err(wyrd_spec::vala::error::BifrostError::OracleRoleUnavailable.into()),
    }
}

/// Requests idempotent cancellation of one active query for the authenticated tenant.
///
/// Once any exact owner accepts cancellation, cancelling this future does not
/// reverse the owner-side transition.
///
/// # Errors
///
/// Returns stable authorization, audit, role-availability, not-found, or conflict errors.
pub async fn cancel_running_query(
    state: &AppState,
    caller: &Caller,
    request_id: RequestId,
) -> Result<CancelRunningQueryResponse, WyrdError> {
    let audit = QueryAuthority::lifecycle(
        state,
        caller,
        "vala.query.running.cancel",
        Some(&request_id),
    );
    audit.authorize().await?;
    audit.record_admission().await?;
    match state.bifrost.query_controls() {
        Some(controls) => controls
            .cancel(caller.data_tenant_id, request_id)
            .await
            .map(|cancelled| CancelRunningQueryResponse {
                request_id: cancelled.request_id,
                cancellation_started: cancelled.cancellation_started,
            }),
        None => Err(wyrd_spec::vala::error::BifrostError::OracleRoleUnavailable.into()),
    }
}

/// operators.
pub(crate) fn arrow_decode_error(error: &QueryIpcDecodeError) -> WyrdError {
    WyrdError::Internal {
        message: "failed to decode Oracle Arrow batch".to_owned(),
        details: serde_json::json!({ "detail": error.to_string() }),
    }
}
/// Preserves the stable terminal error catalog across typed query adapters.
pub(crate) fn terminal_error_to_bifrost(
    code: wyrd_spec::vala::api::QueryTerminalErrorCode,
) -> wyrd_spec::vala::error::BifrostError {
    use wyrd_spec::vala::api::QueryTerminalErrorCode;
    use wyrd_spec::vala::error::BifrostError;

    match code {
        QueryTerminalErrorCode::QueryTimeout => BifrostError::QueryTimeout,
        QueryTerminalErrorCode::QueryVisibilityUnavailable => {
            BifrostError::QueryVisibilityUnavailable
        }
        QueryTerminalErrorCode::QueryTenantInvariant => BifrostError::QueryTenantInvariant,
        QueryTerminalErrorCode::QueryReconciliationInvariant => {
            BifrostError::QueryReconciliationInvariant
        }
        QueryTerminalErrorCode::QueryPeerSecurity => BifrostError::QueryPeerSecurity,
        QueryTerminalErrorCode::QueryAuditUnavailable => BifrostError::QueryAuditUnavailable,
        QueryTerminalErrorCode::CatalogUnreachable => BifrostError::CatalogUnreachable {
            detail: "Oracle typed query catalog unavailable".to_owned(),
        },
        QueryTerminalErrorCode::StorageUnreachable => BifrostError::StorageUnreachable {
            detail: "Oracle typed query storage unavailable".to_owned(),
        },
        QueryTerminalErrorCode::QueryExecutionFailed => BifrostError::QueryExecutionFailed,
    }
}
#[cfg(test)]
mod tests {
    use std::path::Path;
    use wyrd_runtime::permission::PermissionSet;
    use wyrd_runtime::{Principal, PrincipalKind};
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{AuditOutcome, FreshnessPolicy, VisibilityMode};

    use super::*;

    /// Builds one caller with the requested effective permissions.
    ///
    /// # Panics
    ///
    /// Panics when the shared fixture tenant cannot be resolved.
    async fn caller_with(permissions: impl IntoIterator<Item = Permission>) -> Caller {
        let tenant = crate::test_support::test_tenant().await;
        Caller {
            data_tenant_id: tenant,
            principal: Principal::new(
                PrincipalId::new(uuid::Uuid::now_v7()),
                PrincipalKind::User,
                tenant,
                Vec::new(),
                PermissionSet::from_iter(permissions),
            ),
            request_id: RequestId::now_v7(),
            // Nondelegated fixture caller: no verified `act` chain exists.
            delegation_chain: Vec::new(),
        }
    }

    /// Builds application state with real authz audit SQL and no local Oracle role.
    ///
    /// # Panics
    ///
    /// Panics when shared Postgres, storage, or catalog fixtures cannot start.
    async fn state_without_oracle() -> AppState {
        crate::test_support::test_app_state(
            crate::test_support::test_server_postgres().await,
            crate::test_support::test_storage().await,
            crate::test_support::test_catalog().await,
        )
    }

    /// Proves authorization and its denial audit run before local role lookup.
    ///
    /// The allowed caller reaches typed role-unavailable without a Gate, while
    /// the denied caller receives RBAC denial only after the real outbox append
    /// succeeds. Neither path can start Oracle planning or source IO.
    #[test]
    fn bifrost_query_authz_precedes_oracle_role_lookup() {
        wyrd_runtime::runtime().block_on(async {
            let state = state_without_oracle().await;
            let request = BifrostQueryRequest {
                sql: "SELECT 1".to_owned(),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(1_000),
            };
            let allowed = stream_query(
                state.clone(),
                caller_with([Permission::bifrost_query_read()]).await,
                request.clone(),
            )
            .await
            .expect_err("absent Oracle must fail before stream");
            assert_eq!(allowed.status(), 503);

            let denied = stream_query(state, caller_with([]).await, request)
                .await
                .expect_err("under-privileged caller must be denied");
            assert_eq!(
                denied.status(),
                403,
                "authz denial must win over local role availability"
            );
        });
    }

    /// Proves production server query adapters cannot regress to legacy engines or collection.
    #[test]
    fn production_query_paths_have_no_legacy_escape_hatches() {
        let own_source = include_str!("service.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("query service has a production section");
        for (path, complete_source) in [
            ("query/service.rs", own_source),
            ("query/routes.rs", include_str!("routes.rs")),
        ] {
            let source = complete_source
                .split("#[cfg(test)]")
                .next()
                .expect("query adapter has a production section");
            for forbidden in ["application/vnd.apache.arrow.stream", ".collect().await"] {
                assert!(
                    !source.contains(forbidden),
                    "{path} retains forbidden query escape hatch {forbidden}"
                );
            }
        }
    }

    /// Proves the removed async-job SQL owner and initial migration stay absent.
    #[test]
    fn async_query_job_persistence_is_absent() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        for relative in [
            "crates/vala/vala-sql/migrations/20260801000001_olap_query_jobs.sql",
            "crates/vala/vala-sql/src/queries/olap_query_jobs.rs",
            "crates/vala/vala-sql/src/row_types/olap_query_jobs.rs",
            "crates/vala/vala-sql/tests/pg_olap_query_jobs.rs",
        ] {
            assert!(
                !repository.join(relative).exists(),
                "removed async query-job owner returned: {relative}"
            );
        }
    }
}
