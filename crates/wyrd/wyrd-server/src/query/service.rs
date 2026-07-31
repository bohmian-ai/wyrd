//! Auth-bound adapters from Gate transports and typed plans into retained Oracle.

use std::io::Cursor;
use std::time::Instant;

use arrow::ipc::reader::StreamReader;
use arrow::record_batch::RecordBatch;
use futures_util::StreamExt;
use vala_bifrost_redux::oracle::{AuthorizedQueryContext, OracleQueryStream, QueryOptions};
use wyrd_runtime::{Permission, PermissionVerdict};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{
    AuditDecision, AuditResult, AuthMethod, BifrostQueryRequest, QueryStreamFrame,
    QueryTerminalOutcome, VisibilityMode,
};

use crate::AppState;
use crate::audit;
use crate::components::auth::Caller;
use crate::http::error::permission_deny_reason_to_wyrd;
use crate::query::floor;

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
    resource: &'static str,
) -> Result<(), WyrdError> {
    let verdict = state
        .authz
        .permission_check
        .check(&caller.principal, &required);
    match verdict {
        PermissionVerdict::Allow => Ok(()),
        PermissionVerdict::Deny { reason } => {
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
    }
}

/// Converts the authenticated server caller into Oracle's exact context.
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
        Permission::bifrost_query_read().to_string(),
    )
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
    authorize_audited(
        state.clone(),
        caller.clone(),
        Permission::bifrost_query_read(),
        "vala.query.sync",
        "vala.query",
    )
    .await?;
    let context = oracle_context(&caller)?;
    let gate = state
        .bifrost_gate
        .as_ref()
        .cloned()
        .ok_or(wyrd_spec::vala::error::BifrostError::OracleRoleUnavailable)?;
    gate.query_sql(context, request).await.map_err(Into::into)
}

/// Executes one already-lowered typed plan through retained Oracle and collects
/// no more than the typed route's `limit + 1` pagination window.
///
/// # Errors
///
/// Returns Oracle planning/admission/audit, stream protocol, Arrow decoding,
/// timeout, or bounded-result errors.
pub async fn run_typed_query(
    state: &AppState,
    caller: &Caller,
    plan: datafusion::logical_expr::LogicalPlan,
    limit: u32,
) -> Result<(Vec<RecordBatch>, bool), WyrdError> {
    let context = oracle_context(caller)?;
    let gate = state
        .bifrost_gate
        .as_ref()
        .ok_or(wyrd_spec::vala::error::BifrostError::OracleRoleUnavailable)?;
    let deadline = Instant::now()
        .checked_add(floor::SYNC_QUERY_TIMEOUT)
        .ok_or(wyrd_spec::vala::error::BifrostError::QueryTimeout)?;
    let stream = gate
        .query_plan(
            context,
            plan,
            QueryOptions {
                visibility: VisibilityMode::PublishedOnly,
                deadline,
            },
        )
        .await
        .map_err(WyrdError::from)?;
    collect_bounded(stream, limit).await
}

/// Collects one Oracle stream under the typed pagination row and byte ceilings.
///
/// The collector requires exactly one terminal, rejects failed/incomplete
/// streams, and retains at most `limit + 1` rows before producing `has_more`.
///
/// # Errors
///
/// Returns a stable stream, execution, decoding, or result-size error when the
/// logical frame contract or configured bounds are violated.
async fn collect_bounded(
    mut stream: OracleQueryStream,
    limit: u32,
) -> Result<(Vec<RecordBatch>, bool), WyrdError> {
    let row_ceiling = usize::try_from(limit)
        .unwrap_or(usize::MAX)
        .saturating_add(1);
    let mut batches = Vec::new();
    let mut rows = 0_usize;
    let mut encoded_bytes = 0_usize;
    let mut schema_seen = false;
    let mut terminal_seen = false;
    while let Some(frame) = stream.frames.next().await {
        match frame.map_err(WyrdError::from)? {
            QueryStreamFrame::Schema(_) if !schema_seen && batches.is_empty() => {
                schema_seen = true;
            }
            QueryStreamFrame::Batch(batch) if schema_seen && !terminal_seen => {
                if rows >= row_ceiling {
                    continue;
                }
                encoded_bytes = encoded_bytes
                    .checked_add(batch.arrow_ipc_batch.len())
                    .ok_or_else(floor::result_too_large)?;
                if encoded_bytes > floor::MAX_SYNC_RESULT_BYTES {
                    return Err(floor::result_too_large());
                }
                let reader = StreamReader::try_new(Cursor::new(batch.arrow_ipc_batch), None)
                    .map_err(|error| WyrdError::Internal {
                        message: "failed to decode Oracle Arrow batch".to_owned(),
                        details: serde_json::json!({ "detail": error.to_string() }),
                    })?;
                for decoded in reader {
                    let decoded = decoded.map_err(|error| WyrdError::Internal {
                        message: "failed to decode Oracle Arrow batch".to_owned(),
                        details: serde_json::json!({ "detail": error.to_string() }),
                    })?;
                    rows = rows
                        .checked_add(decoded.num_rows())
                        .ok_or_else(floor::result_too_large)?;
                    batches.push(decoded);
                    if rows >= row_ceiling {
                        break;
                    }
                }
            }
            QueryStreamFrame::Terminal(terminal) if schema_seen && !terminal_seen => {
                terminal_seen = true;
                if terminal.outcome == QueryTerminalOutcome::Failed {
                    return Err(wyrd_spec::vala::error::BifrostError::QueryExecutionFailed.into());
                }
            }
            _ => {
                return Err(wyrd_spec::vala::error::BifrostError::QueryStreamProtocol.into());
            }
        }
        if terminal_seen {
            break;
        }
    }
    if !terminal_seen {
        return Err(wyrd_spec::vala::error::BifrostError::QueryStreamIncomplete.into());
    }
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    let has_more = rows > limit;
    Ok((truncate_batches(batches, limit), has_more))
}

/// Truncates already bounded batches without copying their Arrow buffers.
#[must_use]
fn truncate_batches(batches: Vec<RecordBatch>, limit: usize) -> Vec<RecordBatch> {
    let mut result = Vec::with_capacity(batches.len());
    let mut remaining = limit;
    for batch in batches {
        if remaining == 0 {
            break;
        }
        if batch.num_rows() <= remaining {
            remaining -= batch.num_rows();
            result.push(batch);
        } else {
            result.push(batch.slice(0, remaining));
            break;
        }
    }
    result
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
        }
    }

    /// Builds application state with real authz audit SQL and no local Oracle role.
    ///
    /// # Panics
    ///
    /// Panics when shared Postgres, storage, or catalog fixtures cannot start.
    async fn state_without_oracle() -> AppState {
        AppState::new(
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
        for (path, source) in [
            ("query/service.rs", own_source),
            ("query/routes.rs", include_str!("routes.rs")),
            (
                "vala_query/service.rs",
                include_str!("../vala_query/service.rs"),
            ),
            (
                "vala_query/routes.rs",
                include_str!("../vala_query/routes.rs"),
            ),
            ("vala_query/grpc.rs", include_str!("../vala_query/grpc.rs")),
        ] {
            for forbidden in [
                "run_plan_query",
                "provider_with_hot_batches",
                "application/vnd.apache.arrow.stream",
                ".collect().await",
            ] {
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
