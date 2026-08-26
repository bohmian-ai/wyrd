//! Auth-bound adapters from Gate transports and typed plans into retained Oracle.

use std::time::Instant;

use arrow::record_batch::RecordBatch;
use futures_util::StreamExt;
use vala_bifrost_redux::oracle::{
    AuthorizedQueryContext, OracleQueryStream, QueryIpcDecodeError, QueryIpcDecoder, QueryOptions,
};
use wyrd_runtime::{Permission, PermissionVerdict};
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDecision, AuditResult, AuthMethod, BifrostQueryRequest, CancelRunningQueryResponse,
    QueryStreamFrame, QueryTerminalOutcome, RunningQuerySummary, VisibilityMode,
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
    resource: &str,
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
    let deadline = Instant::now()
        .checked_add(floor::SYNC_QUERY_TIMEOUT)
        .ok_or(wyrd_spec::vala::error::BifrostError::QueryTimeout)?;
    let stream = state
        .bifrost
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
    let mut ipc = QueryIpcDecoder::new();
    let mut schema_seen = false;
    let mut terminal_seen = false;
    while let Some(frame) = stream.frames.next().await {
        let frame = match frame {
            Ok(frame) => frame,
            Err(error) => {
                stream.cancel().await;
                return Err(WyrdError::from(error));
            }
        };
        match frame {
            QueryStreamFrame::Schema(schema) if !schema_seen && batches.is_empty() => {
                if let Err(error) = ipc.accept_schema(&schema.arrow_ipc_schema) {
                    stream.cancel().await;
                    return Err(arrow_decode_error(&error));
                }
                schema_seen = true;
            }
            QueryStreamFrame::Batch(batch) if schema_seen && !terminal_seen => {
                let retaining = rows < row_ceiling;
                if retaining {
                    // The byte ceiling is charged before decoding so an
                    // oversized fragment is refused without expanding it.
                    encoded_bytes = match encoded_bytes.checked_add(batch.arrow_ipc_batch.len()) {
                        Some(bytes) => bytes,
                        None => {
                            stream.cancel().await;
                            return Err(floor::result_too_large());
                        }
                    };
                    if encoded_bytes > floor::MAX_SYNC_RESULT_BYTES {
                        stream.cancel().await;
                        return Err(floor::result_too_large());
                    }
                }
                // Every retained-or-not fragment is still fed: the decoder is
                // stateful, so skipping one would corrupt the remaining stream
                // and the terminal end-of-stream delta.
                let decoded = match ipc.accept_batch(&batch.arrow_ipc_batch) {
                    Ok(decoded) => decoded,
                    Err(error) => {
                        stream.cancel().await;
                        return Err(arrow_decode_error(&error));
                    }
                };
                if !retaining {
                    continue;
                }
                rows = match rows.checked_add(decoded.num_rows()) {
                    Some(rows) => rows,
                    None => {
                        stream.cancel().await;
                        return Err(floor::result_too_large());
                    }
                };
                batches.push(decoded);
            }
            QueryStreamFrame::Terminal(terminal) if schema_seen && !terminal_seen => {
                terminal_seen = true;
                if terminal.outcome == QueryTerminalOutcome::Failed {
                    let error = terminal.error.as_ref().map_or(
                        wyrd_spec::vala::error::BifrostError::QueryExecutionFailed,
                        |error| terminal_error_to_bifrost(error.code),
                    );
                    return Err(error.into());
                }
                if let Err(error) = ipc.accept_eos(&terminal.arrow_ipc_eos) {
                    return Err(arrow_decode_error(&error));
                }
            }
            _ => {
                stream.cancel().await;
                return Err(wyrd_spec::vala::error::BifrostError::QueryStreamProtocol.into());
            }
        }
        if terminal_seen {
            break;
        }
    }
    if !terminal_seen {
        stream.cancel().await;
        return Err(wyrd_spec::vala::error::BifrostError::QueryStreamIncomplete.into());
    }
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    let has_more = rows > limit;
    Ok((truncate_batches(batches, limit), has_more))
}

/// Projects a query IPC decoding refusal onto the stable internal error shape.
///
/// Decoding failures are server-side stream defects, not caller errors, so they
/// keep the existing internal projection while retaining the closed reason for
/// operators.
fn arrow_decode_error(error: &QueryIpcDecodeError) -> WyrdError {
    WyrdError::Internal {
        message: "failed to decode Oracle Arrow batch".to_owned(),
        details: serde_json::json!({ "detail": error.to_string() }),
    }
}

/// Preserves the stable terminal error catalog across typed query adapters.
fn terminal_error_to_bifrost(
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tokio_util::sync::CancellationToken;
    use wyrd_runtime::permission::PermissionSet;
    use wyrd_runtime::{Principal, PrincipalKind};
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{
        FreshnessPolicy, QueryBatchFrame, QueryFreshness, QuerySchemaFrame, QuerySource,
        QueryStreamFrame, QueryTerminalFrame, SourceCompletion, SourceCompletionOutcome,
        VisibilityMode,
    };

    use super::*;

    /// Encodes one split query IPC stream exactly as the Oracle encoder does.
    ///
    /// Returns the schema fragment, one bare fragment per written batch, and the
    /// end-of-stream fragment the terminal frame carries.
    ///
    /// # Panics
    ///
    /// Panics when the fixture batches cannot be encoded.
    fn split_ipc_stream(batches: &[&[i64]]) -> (Vec<u8>, Vec<Vec<u8>>, Vec<u8>) {
        use arrow::array::Int64Array;
        use arrow::datatypes::{DataType, Field, Schema};

        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let mut writer = arrow::ipc::writer::StreamWriter::try_new(Vec::new(), schema.as_ref())
            .expect("schema writer starts");
        let prefix = std::mem::take(writer.get_mut());
        let fragments = batches
            .iter()
            .map(|values| {
                let batch = RecordBatch::try_new(
                    Arc::clone(&schema),
                    vec![Arc::new(Int64Array::from(values.to_vec()))],
                )
                .expect("fixture batch is valid");
                writer.write(&batch).expect("fixture batch writes");
                std::mem::take(writer.get_mut())
            })
            .collect();
        writer.finish().expect("fixture writer finishes");
        (prefix, fragments, std::mem::take(writer.get_mut()))
    }

    /// Synthetic owner probe for admission release and fenced-tail cleanup.
    #[derive(Default)]
    struct CollectorOwnerProbe {
        /// Number of local slots retained by the owner.
        slots_in_use: AtomicUsize,
        /// Whether durable release completed before collection returned.
        release_complete: AtomicBool,
        /// Whether fenced tail state completed release.
        fence_released: AtomicBool,
    }

    impl CollectorOwnerProbe {
        /// Releases local slots and marks durable/fenced cleanup complete.
        async fn release(&self) {
            tokio::task::yield_now().await;
            self.slots_in_use.store(0, Ordering::Release);
            self.fence_released.store(true, Ordering::Release);
            self.release_complete.store(true, Ordering::Release);
        }
    }

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
            let source = complete_source
                .split("#[cfg(test)]")
                .next()
                .expect("query adapter has a production section");
            for forbidden in [
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

    /// The collector decodes one split IPC stream per query and requires its close.
    ///
    /// Bounded collection is the server's own consumer of the public stream, so
    /// it must hold one stateful decoder for the whole query: a schema fragment
    /// once, bare deltas per batch, and the terminal end-of-stream that proves
    /// the stream closed rather than truncated.
    #[test]
    fn stateful_ipc_decoder_contract() {
        wyrd_runtime::runtime().block_on(async {
            let (prefix, fragments, eos) = split_ipc_stream(&[&[1, 2], &[3], &[4, 5]]);
            let terminal = |arrow_ipc_eos: Vec<u8>| QueryTerminalFrame {
                outcome: QueryTerminalOutcome::Success,
                freshness: QueryFreshness::Complete,
                row_count: 5,
                warnings: Vec::new(),
                source_completion: vec![
                    SourceCompletion {
                        source: QuerySource::Iceberg,
                        outcome: SourceCompletionOutcome::Complete,
                    },
                    SourceCompletion {
                        source: QuerySource::HotSealed,
                        outcome: SourceCompletionOutcome::Complete,
                    },
                ],
                error: None,
                arrow_ipc_eos,
            };
            let frames = |arrow_ipc_eos: Vec<u8>| {
                let mut frames = vec![Ok(QueryStreamFrame::Schema(QuerySchemaFrame {
                    schema_fingerprint: "test".to_owned(),
                    arrow_ipc_schema: prefix.clone(),
                }))];
                frames.extend(fragments.iter().map(|fragment| {
                    Ok(QueryStreamFrame::Batch(QueryBatchFrame {
                        arrow_ipc_batch: fragment.clone(),
                    }))
                }));
                frames.push(Ok(QueryStreamFrame::Terminal(terminal(arrow_ipc_eos))));
                frames
            };
            let stream = |frames: Vec<_>| {
                OracleQueryStream::test_new(
                    "test".to_owned(),
                    Box::pin(async_stream::stream! {
                        for frame in frames {
                            yield frame;
                        }
                    }),
                    CancellationToken::new(),
                )
            };

            let (batches, has_more) = collect_bounded(stream(frames(eos.clone())), 100)
                .await
                .expect("split stream collects");
            assert_eq!(
                batches.len(),
                3,
                "each fragment decodes to exactly one batch"
            );
            assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 5);
            assert!(!has_more);

            let (bounded, has_more) = collect_bounded(stream(frames(eos)), 2)
                .await
                .expect("bounded collection still consumes the whole stream");
            assert_eq!(
                bounded.iter().map(RecordBatch::num_rows).sum::<usize>(),
                2,
                "row ceiling truncates without abandoning decoder state"
            );
            assert!(has_more);

            assert!(
                collect_bounded(stream(frames(Vec::new())), 100)
                    .await
                    .is_err(),
                "a successful terminal without its end-of-stream is refused"
            );
        });
    }

    /// Proves every pre-terminal collector failure actively cancels its stream.
    #[test]
    fn collector_failures_cancel_stream_cleanup() {
        wyrd_runtime::runtime().block_on(async {
            let (prefix, _fragments, _eos) = split_ipc_stream(&[&[1, 2]]);
            let schema_frame = QueryStreamFrame::Schema(QuerySchemaFrame {
                schema_fingerprint: "test".to_owned(),
                arrow_ipc_schema: prefix,
            });
            let cases = [
                (
                    "oversized",
                    vec![
                        Ok(schema_frame.clone()),
                        Ok(QueryStreamFrame::Batch(QueryBatchFrame {
                            arrow_ipc_batch: vec![0; floor::MAX_SYNC_RESULT_BYTES + 1],
                        })),
                    ],
                ),
                (
                    "malformed",
                    vec![
                        Ok(schema_frame.clone()),
                        Ok(QueryStreamFrame::Batch(QueryBatchFrame {
                            arrow_ipc_batch: vec![1, 2, 3],
                        })),
                    ],
                ),
                (
                    "unreadable schema",
                    vec![Ok(QueryStreamFrame::Schema(QuerySchemaFrame {
                        schema_fingerprint: "test".to_owned(),
                        arrow_ipc_schema: Vec::new(),
                    }))],
                ),
                (
                    "protocol",
                    vec![Ok(QueryStreamFrame::Batch(QueryBatchFrame {
                        arrow_ipc_batch: vec![1],
                    }))],
                ),
                ("incomplete", vec![Ok(schema_frame)]),
            ];
            for (name, frames) in cases {
                let cancellation = CancellationToken::new();
                let owner = Arc::new(CollectorOwnerProbe {
                    slots_in_use: AtomicUsize::new(1),
                    ..CollectorOwnerProbe::default()
                });
                let owner_for_stream = Arc::clone(&owner);
                let frames = async_stream::stream! {
                    for frame in frames {
                        yield frame;
                    }
                    owner_for_stream.release().await;
                };
                let stream = OracleQueryStream::test_new(
                    "test".to_owned(),
                    Box::pin(frames),
                    cancellation.clone(),
                );
                assert!(
                    collect_bounded(stream, 100).await.is_err(),
                    "{name} must fail"
                );
                assert!(cancellation.is_cancelled(), "{name} must cancel cleanup");
                assert_eq!(owner.slots_in_use.load(Ordering::Acquire), 0);
                assert!(owner.release_complete.load(Ordering::Acquire));
                assert!(owner.fence_released.load(Ordering::Acquire));
            }
        });
    }
}
