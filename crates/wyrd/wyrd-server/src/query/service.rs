//! Auth-bound adapters from Gate transports and typed plans into retained Oracle.

use vala_bifrost_redux::oracle::{AuthorizedQueryContext, OracleQueryStream, QueryIpcDecodeError};
use wyrd_runtime::{Action, Permission, PermissionDenyReason, PermissionVerdict, Resource};
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDecision, AuditResult, AuthMethod, BifrostQueryRequest, CancelRunningQueryResponse,
    RunningQuerySummary,
};

use crate::AppState;
use crate::audit;
use crate::components::auth::Caller;
use crate::http::error::permission_deny_reason_to_wyrd;

/// Authorizes one query permission and durably audits a denial.
///
/// Successful authorization deliberately writes no read-decision row here:
/// retained Oracle commits the immutable visibility decision after planning and
/// before any query data is read.
///
/// # Errors
///
/// Returns the stable authorization denial or audit-unavailable error.
pub(crate) async fn authorize_audited(
    state: AppState,
    caller: Caller,
    required: Permission,
    operation: &'static str,
    resource: &str,
) -> Result<(), WyrdError> {
    let verdict = state
        .authz
        .permission_check
        .check(&caller.principal, &required);
    match verdict {
        PermissionVerdict::Allow => Ok(()),
        PermissionVerdict::Deny { reason } => {
            record_denial(state, caller, &required, operation, resource, reason).await
        }
    }
}

/// Admits one query request that holds the Bifrost read capability at all.
///
/// This is deliberately coarse. A schema- or table-scoped grant cannot satisfy
/// an object-wide requirement, so the old exact-global prerequisite would have
/// rejected every scoped principal at the route before Oracle ever resolved
/// which tables the SQL touches. The route therefore admits any principal
/// holding `bifrost_query:read` under *some* scope, and the authoritative
/// per-object decision is taken inside Oracle against the resolved scan set.
///
/// # Errors
///
/// Returns the stable authorization denial, or audit-unavailable when the
/// denial cannot be durably recorded.
async fn admit_query_capability(
    state: AppState,
    caller: Caller,
    operation: &'static str,
    resource: &str,
) -> Result<(), WyrdError> {
    if caller
        .principal
        .effective_permissions
        .covers_operation(&Resource::BifrostQuery, &Action::Read)
    {
        return Ok(());
    }
    let required = Permission::bifrost_query_read();
    let reason = PermissionDenyReason::Rbac {
        required: Box::new(required.clone()),
        principal: caller.principal.id,
    };
    record_denial(state, caller, &required, operation, resource, reason).await
}

/// Durably records one authorization denial before it reaches the caller.
///
/// Every refusal on this path fails closed on the audit append: a denial that
/// cannot be recorded is returned as audit-unavailable rather than as a plain
/// rejection, so no refusal is silently unlogged.
///
/// # Errors
///
/// Returns audit-unavailable when the tenant outbox append fails, and otherwise
/// the stable authorization denial built from `reason`.
async fn record_denial(
    state: AppState,
    caller: Caller,
    required: &Permission,
    operation: &'static str,
    resource: &str,
    reason: PermissionDenyReason,
) -> Result<(), WyrdError> {
    let event = audit::audit_event(
        &caller,
        operation,
        resource,
        &required.to_string(),
        AuditDecision::Deny,
        AuditResult::Failure,
        "rbac permission denied",
    );
    audit::record_audit_owned(
        state.postgres.vala_pool().clone(),
        caller.data_tenant_id,
        event,
    )
    .await?;
    Err(permission_deny_reason_to_wyrd(reason))
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
    admit_query_capability(
        state.clone(),
        caller.clone(),
        "vala.query.sync",
        "vala.query",
    )
    .await?;
    let context = oracle_context(&caller)?;
    state
        .bifrost
        .query_sql(context, request)
        .await
        .map_err(Into::into)
}

/// Audit owner for one authenticated live-query control operation.
struct ControlAudit<'a> {
    /// Shared server state containing authorization and the durable audit outbox.
    state: &'a AppState,
    /// Authenticated caller whose tenant and request spine own the audit row.
    caller: &'a Caller,
    /// Stable operation name for the public lifecycle action.
    operation: &'static str,
    /// Redacted lifecycle resource, optionally qualified by target request ID.
    resource: String,
    /// Existing permission required by every public lifecycle action.
    permission: Permission,
}

impl<'a> ControlAudit<'a> {
    /// Binds one lifecycle operation to its authenticated caller and redacted target.
    fn new(
        state: &'a AppState,
        caller: &'a Caller,
        operation: &'static str,
        request_id: Option<&RequestId>,
    ) -> Self {
        let resource = request_id.map_or_else(
            || "vala.query.lifecycle".to_owned(),
            |request_id| format!("vala.query.lifecycle/{request_id}"),
        );
        Self {
            state,
            caller,
            operation,
            resource,
            permission: Permission::bifrost_query_read(),
        }
    }

    /// Authorizes the lifecycle action, durably recording any denial before dispatch.
    ///
    /// # Errors
    ///
    /// Returns the stable authorization denial or audit-unavailable error. A
    /// failed denial append prevents the lifecycle owner from being called.
    async fn authorize(&self) -> Result<(), WyrdError> {
        authorize_audited(
            self.state.clone(),
            self.caller.clone(),
            self.permission.clone(),
            self.operation,
            &self.resource,
        )
        .await
    }

    /// Durably records the truthful result returned by the lifecycle owner.
    ///
    /// The row contains only lifecycle identity and outcome metadata. Query
    /// text, parameters, and results never enter the audit record.
    ///
    /// # Errors
    ///
    /// Returns audit-unavailable when the tenant outbox append cannot commit;
    /// callers must then fail closed instead of returning the control result.
    async fn record<T>(&self, outcome: &Result<T, WyrdError>) -> Result<(), WyrdError> {
        let (result, payload_summary) = if outcome.is_ok() {
            (AuditResult::Success, "running query control succeeded")
        } else {
            (AuditResult::Failure, "running query control failed")
        };
        let event = audit::audit_event(
            self.caller,
            self.operation,
            &self.resource,
            &self.permission.to_string(),
            AuditDecision::Allow,
            result,
            payload_summary,
        );
        audit::record_audit_owned(
            self.state.postgres.vala_pool().clone(),
            self.caller.data_tenant_id,
            event,
        )
        .await
    }

    /// Durably gates one mutating cancellation before owner dispatch.
    ///
    /// This records authorization to attempt dispatch under a distinct operation;
    /// it does not claim that cancellation succeeded. [`Self::record`] writes the
    /// truthful owner outcome after dispatch.
    ///
    /// # Errors
    ///
    /// Returns audit-unavailable before the cancellation owner is called.
    async fn record_cancel_attempt(&self) -> Result<(), WyrdError> {
        #[cfg(feature = "test-support")]
        if self
            .state
            .query_control_audit_fault
            .as_ref()
            .is_some_and(crate::state::QueryControlAuditFaultController::cancel_attempts_fail)
        {
            return Err(wyrd_spec::vala::error::BifrostError::AuditUnavailable {
                detail: "cancellation audit gate unavailable before dispatch".to_owned(),
            }
            .into());
        }
        let event = audit::audit_event(
            self.caller,
            "vala.query.running.cancel.attempt",
            &self.resource,
            &self.permission.to_string(),
            AuditDecision::Allow,
            AuditResult::Success,
            "running query cancellation dispatch authorized",
        );
        audit::record_audit_owned(
            self.state.postgres.vala_pool().clone(),
            self.caller.data_tenant_id,
            event,
        )
        .await
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
    let audit = ControlAudit::new(state, caller, "vala.query.running.list", None);
    audit.authorize().await?;
    let outcome = match state.bifrost.query_controls() {
        Some(controls) => controls.list(caller.data_tenant_id).await,
        None => Err(wyrd_spec::vala::error::BifrostError::OracleRoleUnavailable.into()),
    };
    audit.record(&outcome).await?;
    outcome
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
    let audit = ControlAudit::new(state, caller, "vala.query.running.get", Some(&request_id));
    audit.authorize().await?;
    let outcome = match state.bifrost.query_controls() {
        Some(controls) => controls.get(caller.data_tenant_id, request_id).await,
        None => Err(wyrd_spec::vala::error::BifrostError::OracleRoleUnavailable.into()),
    };
    audit.record(&outcome).await?;
    outcome
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
    let audit = ControlAudit::new(
        state,
        caller,
        "vala.query.running.cancel",
        Some(&request_id),
    );
    audit.authorize().await?;
    audit.record_cancel_attempt().await?;
    let outcome = match state.bifrost.query_controls() {
        Some(controls) => controls
            .cancel(caller.data_tenant_id, request_id)
            .await
            .map(|cancelled| CancelRunningQueryResponse {
                request_id: cancelled.request_id,
                cancellation_started: cancelled.cancellation_started,
            }),
        None => Err(wyrd_spec::vala::error::BifrostError::OracleRoleUnavailable.into()),
    };
    if audit.record(&outcome).await.is_err() {
        return Err(wyrd_spec::vala::error::BifrostError::AuditUnavailable {
            detail: "cancellation produced an outcome but its audit append failed".to_owned(),
        }
        .into());
    }
    outcome
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
    use wyrd_spec::vala::api::{FreshnessPolicy, VisibilityMode};

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
