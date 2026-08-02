//! RBAC-gated query service functions: the sync (raw Arrow IPC) path and the
//! async (validate-before-enqueue) path.
//!
//! Both paths authorize `bifrost_query_read` and run the syntactic floor
//! (`floor::validate_query_sql`) *before* any provider is built, any query is
//! planned, or any durable `olap_query_jobs` row is written — so an invalid SQL
//! async submission is a `400` that leaves no queued row. Providers are built
//! per request and scoped to the caller's tenant via `state.bifrost.provider`;
//! there is never a shared `SessionContext`. Engine and storage errors cross the
//! public boundary through the one `WyrdError` delegate rendered by the single
//! `WyrdErrorResponse`.

use std::sync::Arc;

use arrow::datatypes::Schema;
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use datafusion::common::TableReference;
use datafusion::datasource::MemTable;
use datafusion::error::DataFusionError;
use datafusion::prelude::SessionContext;
use serde_json::{Value as JsonValue, json};
use vala_bifrost::error::BifrostError as EngineBifrostError;
use vala_bifrost::session::{
    wyrd_session_context, wyrd_session_context_with_memory, wyrd_session_context_with_pool,
};
use vala_bifrost::{BifrostNamespace, SchemaFingerprint};
use vala_bifrost_redux::catalog::BifrostCatalogError as ReduxCatalogError;
use vala_bifrost_redux::catalog::TableRef as ReduxTableRef;
use vala_bifrost_redux::namespaces::BifrostNamespace as ReduxNamespace;
use vala_bifrost_redux::tables::builtin_table;
use vala_sql::TenantConn;
use vala_sql::queries::olap_query_jobs::{NewQueryJob, enqueue_query_job, query_job_status};
use wyrd_runtime::{Permission, PermissionVerdict};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::BifrostError as ValaError;
use wyrd_spec::vala::api::{
    AsyncJobState, AsyncQueryRequest, AsyncQueryResponse, AsyncQueryStatus, AuditDecision,
    AuditResult, ExecutorAvailability, JobUid, QueryParam, SyncQueryRequest,
};

use crate::AppState;
use crate::audit;
use crate::bifrost::convert;
use crate::components::auth::Caller;
use crate::http::error::permission_deny_reason_to_wyrd;
use crate::query::floor;
use crate::vala_query::service::fetch_hot_batches;

/// Materialized sync-query result: the Arrow IPC stream plus the header metadata
/// the axum adapter stamps onto the response.
#[derive(Debug)]
pub struct SyncQueryResult {
    /// Serialized `application/vnd.apache.arrow.stream` body.
    pub body: Vec<u8>,
    /// Lower-case hex of the result schema fingerprint (`X-Wyrd-Schema-Fingerprint`).
    pub schema_fingerprint: String,
    /// Total result row count across all batches (`X-Wyrd-Row-Count`).
    pub row_count: usize,
}

/// Authorize the caller against a required permission, auditing a denial.
///
/// `pub(crate)` so typed-route modules can reuse this without duplicating the
/// audit-deny logic.
///
/// On success no row is written here — the handler audits the executed op. On
/// denial a `decision = deny` row is appended in its own transaction (M-06
/// doctrine: every allow AND every deny is audited) before the `403` propagates.
pub(crate) async fn authorize_audited(
    state: &AppState,
    caller: &Caller,
    required: Permission,
    operation: &str,
    resource: &str,
) -> Result<(), WyrdError> {
    match state
        .authz
        .permission_check
        .check(&caller.principal, &required)
    {
        PermissionVerdict::Allow => Ok(()),
        PermissionVerdict::Deny { reason } => {
            let event = audit::audit_event(
                caller,
                operation,
                resource,
                &required.to_string(),
                AuditDecision::Deny,
                AuditResult::Failure,
                "rbac permission denied",
            );
            audit::record_audit(state.postgres.vala_pool(), caller.data_tenant_id, &event).await?;
            Err(permission_deny_reason_to_wyrd(reason))
        }
    }
}

/// Map an engine Bifrost error to the public `WyrdError` via the single delegate.
fn map_engine_error(error: EngineBifrostError) -> WyrdError {
    error.into_public().into()
}

/// Map a query-job storage failure to the public catalog-unreachable code, never
/// leaking the underlying SQL/connection detail across the boundary.
fn storage_error(error: impl std::fmt::Display) -> WyrdError {
    tracing::error!(error = %error, "query-job storage operation failed");
    ValaError::CatalogUnreachable {
        detail: "query-job storage operation failed".to_owned(),
    }
    .into()
}

/// Map a DataFusion planning/execution error to the public boundary.
///
/// A planning-stage rejection (parse/plan/schema) of a floor-valid statement is a
/// `400 QUERY_INVALID_SQL` — the floor passed the syntax but planning found an
/// unknown table/column or unsupported construct. Anything else is a `500`.
fn map_datafusion_error(error: DataFusionError) -> WyrdError {
    if matches!(
        error,
        DataFusionError::SQL(..) | DataFusionError::Plan(_) | DataFusionError::SchemaError(..)
    ) {
        return floor::invalid_sql(format!("query planning rejected the statement: {error}"));
    }
    WyrdError::Internal {
        message: "query execution failed".to_owned(),
        details: json!({ "detail": error.to_string() }),
    }
}

/// Map an Arrow IPC encoding failure to a `500`.
fn ipc_error(error: arrow::error::ArrowError) -> WyrdError {
    WyrdError::Internal {
        message: "failed to encode query result as Arrow IPC".to_owned(),
        details: json!({ "detail": error.to_string() }),
    }
}

/// Register one tenant-qualified Redux provider for a SQL table reference.
///
/// The provider includes the bounded Scribe hot tail, matching the typed Vala
/// query service. A missing physical table receives its built-in empty schema
/// when available; no Oracle snapshot or publication fence is required.
///
/// # Errors
///
/// Returns [`WyrdError`] when namespace resolution, Scribe hot-read, provider
/// construction, or DataFusion registration fails.
///
/// # Cancellation
///
/// Cancellation abandons read-only SQL, Scribe, Iceberg, and DataFusion setup
/// without durable effects.
async fn register_redux_provider(
    ctx: &SessionContext,
    state: &AppState,
    ns: BifrostNamespace,
    name: &str,
    tenant: wyrd_spec::ids::DataTenantId,
    fqn: &str,
) -> Result<(), WyrdError> {
    let segment = ns.as_str().strip_prefix("vala.").unwrap_or(ns.as_str());
    let redux_ns =
        ReduxNamespace::from_domain_namespace(segment).ok_or_else(|| WyrdError::Internal {
            message: "unknown Redux Bifrost namespace".to_owned(),
            details: json!({ "namespace": ns.as_str() }),
        })?;
    let table = ReduxTableRef::new(redux_ns, name);
    let hot_batches = fetch_hot_batches(state, &table, tenant, None, None).await?;
    let catalog = state
        .bifrost_redux
        .as_deref()
        .ok_or_else(|| WyrdError::Internal {
            message: "Redux Bifrost catalog is not configured".to_owned(),
            details: JsonValue::Null,
        })?;
    match catalog
        .provider_with_hot_batches(&table, tenant, hot_batches)
        .await
    {
        Ok(provider) => ctx
            .register_table(TableReference::bare(fqn), Arc::new(provider))
            .map_err(map_datafusion_error)
            .map(|_| ()),
        Err(ReduxCatalogError::TableNotFound(_)) => {
            let Some(definition) = builtin_table(segment, name) else {
                return Ok(());
            };
            let empty = MemTable::try_new((definition.schema)(), vec![vec![]])
                .map_err(map_datafusion_error)?;
            ctx.register_table(TableReference::bare(fqn), Arc::new(empty))
                .map_err(map_datafusion_error)
                .map(|_| ())
        }
        Err(error) => Err(WyrdError::Internal {
            message: "bifrost provider error".to_owned(),
            details: json!({ "detail": error.to_string() }),
        }),
    }
}

/// Build a tenant-scoped `SessionContext` and register only the Bifrost tables the
/// query references. The context carries the tenant analyzer rule and each
/// provider enforces the physical tenant boundary; there is never a shared or
/// cross-tenant context. Names that do not resolve to a Bifrost table are skipped.
async fn build_tenant_session(
    state: &AppState,
    caller: &Caller,
    sql: &str,
) -> Result<SessionContext, WyrdError> {
    let ctx = if let Some(pool) = &state.bifrost_query_memory {
        wyrd_session_context_with_pool(caller.data_tenant_id, pool.clone())
            .map_err(map_datafusion_error)?
    } else if let Some(memory) = &state.bifrost_memory {
        wyrd_session_context_with_memory(caller.data_tenant_id, memory.bifrost_limit_bytes() / 4)
            .map_err(map_datafusion_error)?
    } else {
        wyrd_session_context(caller.data_tenant_id)
    };
    for fqn in floor::referenced_tables(sql) {
        let Some((ns, name)) = BifrostNamespace::split_fqn(&fqn) else {
            continue;
        };
        if state.bifrost_redux.is_some() {
            register_redux_provider(&ctx, state, ns, &name, caller.data_tenant_id, &fqn).await?;
        } else {
            match state
                .bifrost
                .provider(ns, &name, caller.data_tenant_id)
                .await
            {
                Ok(provider) => {
                    ctx.register_table(TableReference::bare(fqn), Arc::new(provider))
                        .map_err(map_datafusion_error)?;
                }
                Err(EngineBifrostError::TableNotFound(_)) => continue,
                Err(other) => return Err(map_engine_error(other)),
            }
        }
    }
    Ok(ctx)
}

/// Encode a batch stream as a single Arrow IPC stream body.
fn encode_ipc(schema: &Schema, batches: &[RecordBatch]) -> Result<Vec<u8>, WyrdError> {
    let mut buffer = Vec::new();
    {
        let mut writer = StreamWriter::try_new(&mut buffer, schema).map_err(ipc_error)?;
        for batch in batches {
            writer.write(batch).map_err(ipc_error)?;
        }
        writer.finish().map_err(ipc_error)?;
    }
    Ok(buffer)
}

/// Run a synchronous SELECT and return its Arrow IPC stream.
///
/// Order (each gate before the next): authorize `bifrost_query_read` → syntactic
/// floor → per-request tenant-scoped providers → DataFusion exec under a wall-clock
/// budget → row/byte ceilings → IPC encode. A timeout is `504 QUERY_TIMEOUT`; an
/// over-ceiling result is `413 QUERY_RESULT_TOO_LARGE`.
pub async fn run_sync_query(
    state: &AppState,
    caller: Caller,
    body: SyncQueryRequest,
) -> Result<SyncQueryResult, WyrdError> {
    let started = std::time::Instant::now();
    let span = tracing::info_span!("bifrost.query.execute", result = tracing::field::Empty);
    let result =
        tracing::Instrument::instrument(run_sync_query_inner(state, caller, body), span.clone())
            .await;
    let outcome = match &result {
        Ok(_) => "success",
        Err(
            WyrdError::Internal { .. }
            | WyrdError::UpstreamFailure { .. }
            | WyrdError::Timeout { .. },
        ) => "failed",
        Err(_) => "rejected",
    };
    metrics::histogram!(
        "bifrost_query_duration_seconds",
        "result" => outcome
    )
    .record(started.elapsed().as_secs_f64());
    span.record("result", outcome);
    result
}

/// Execute the authorized Bifrost query after the public telemetry boundary.
///
/// # Errors
///
/// Returns authorization, validation, DataFusion, result-ceiling, encoding, or
/// audit failures through the public query contract.
async fn run_sync_query_inner(
    state: &AppState,
    caller: Caller,
    body: SyncQueryRequest,
) -> Result<SyncQueryResult, WyrdError> {
    authorize_audited(
        state,
        &caller,
        Permission::bifrost_query_read(),
        "vala.query.sync",
        "vala.query",
    )
    .await?;
    floor::validate_query_sql(&body.sql)?;

    let ctx = build_tenant_session(state, &caller, &body.sql).await?;

    let sql = body.sql.clone();
    let (schema, batches) = tokio::time::timeout(floor::SYNC_QUERY_TIMEOUT, async {
        let df = ctx.sql(&sql).await?;
        let schema = df.schema().as_arrow().clone();
        let batches = df.collect().await?;
        Ok::<_, DataFusionError>((schema, batches))
    })
    .await
    .map_err(|_| floor::query_timeout())?
    .map_err(map_datafusion_error)?;

    let row_count: usize = batches.iter().map(RecordBatch::num_rows).sum();
    if row_count > floor::MAX_SYNC_RESULT_ROWS {
        return Err(floor::result_too_large());
    }

    let body_bytes = encode_ipc(&schema, &batches)?;
    if body_bytes.len() > floor::MAX_SYNC_RESULT_BYTES {
        return Err(floor::result_too_large());
    }

    let schema_fingerprint = convert::to_hex(&SchemaFingerprint::from_arrow_schema(&schema).0);

    // Audit the authorized, successful read in its own transaction — committed
    // BEFORE the Arrow body streams to the client, so a durable record of the
    // query precedes any result leaving the server.
    let event = audit::audit_event(
        &caller,
        "vala.query.sync",
        "vala.query",
        &Permission::bifrost_query_read().to_string(),
        AuditDecision::Allow,
        AuditResult::Success,
        "sync select executed",
    );
    audit::record_audit(state.postgres.vala_pool(), caller.data_tenant_id, &event).await?;

    Ok(SyncQueryResult {
        body: body_bytes,
        schema_fingerprint,
        row_count,
    })
}

/// Redact bound parameters to type tags only. Raw literals never reach durable
/// storage — the persisted `params_redacted` records the shape, not the values.
fn redact_params(params: &[QueryParam]) -> JsonValue {
    let tags: Vec<JsonValue> = params
        .iter()
        .map(|param| {
            let tag = match param {
                QueryParam::Null => "null",
                QueryParam::Bool(_) => "bool",
                QueryParam::Int(_) => "int",
                QueryParam::Float(_) => "float",
                QueryParam::Text(_) => "text",
            };
            json!({ "type": tag })
        })
        .collect();
    JsonValue::Array(tags)
}

/// Derive a per-tenant idempotency key from the normalized SQL and redacted param
/// shape, so a retried identical submission de-duplicates to the same job row.
fn idempotency_key(sql_normalized: &str, params_redacted: &JsonValue) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(sql_normalized.as_bytes());
    hasher.update([0u8]);
    hasher.update(params_redacted.to_string().as_bytes());
    convert::to_hex(&hasher.finalize())
}

/// Validate then enqueue an async query job.
///
/// The syntactic floor runs *before* `enqueue_query_job`: an invalid SQL
/// submission returns `400 QUERY_INVALID_SQL` and writes no durable row. A valid
/// submission lands exactly one `queued` row (idempotent on retry) and reports
/// `executor_availability = PendingStage5` — the executor does not exist until
/// Stage 5.
pub async fn submit_async_query(
    state: &AppState,
    caller: Caller,
    body: AsyncQueryRequest,
) -> Result<AsyncQueryResponse, WyrdError> {
    authorize_audited(
        state,
        &caller,
        Permission::bifrost_query_read(),
        "vala.query.async.submit",
        "vala.query",
    )
    .await?;
    floor::validate_query_sql(&body.sql)?;

    let sql_normalized = body.sql.trim().to_owned();
    let params_redacted = redact_params(&body.params);
    let key = idempotency_key(&sql_normalized, &params_redacted);

    let mut conn = TenantConn::acquire(state.postgres.vala_pool(), caller.data_tenant_id)
        .await
        .map_err(storage_error)?;
    let job_uid = enqueue_query_job(
        &mut conn,
        NewQueryJob {
            idempotency_key: &key,
            sql_normalized: &sql_normalized,
            params_redacted: &params_redacted,
        },
    )
    .await
    .map_err(storage_error)?;
    // Append the audit row in the SAME tx as the `olap_query_jobs` write, so the
    // enqueued job and its audit record commit atomically (fail-closed).
    let event = audit::audit_event(
        &caller,
        "vala.query.async.submit",
        "vala.query",
        &Permission::bifrost_query_read().to_string(),
        AuditDecision::Allow,
        AuditResult::Success,
        "async query enqueued",
    );
    audit::append_on(&mut conn, &event).await?;
    conn.commit().await.map_err(storage_error)?;

    Ok(AsyncQueryResponse {
        job_uid,
        state: AsyncJobState::Queued,
        executor_availability: ExecutorAvailability::PendingStage5,
    })
}

/// Read one async query job's status under the caller's tenant bind. A job owned
/// by another tenant is invisible under RLS and surfaces as `404`.
pub async fn get_async_query_status(
    state: &AppState,
    caller: Caller,
    job_uid: JobUid,
) -> Result<AsyncQueryStatus, WyrdError> {
    authorize_audited(
        state,
        &caller,
        Permission::bifrost_query_read(),
        "vala.query.async.status",
        "vala.query.async",
    )
    .await?;

    let mut conn = TenantConn::acquire(state.postgres.vala_pool(), caller.data_tenant_id)
        .await
        .map_err(storage_error)?;
    let status = query_job_status(&mut conn, job_uid)
        .await
        .map_err(storage_error)?;
    // Audit the authorized status read in the SAME tx as the job read. Recorded
    // whether or not the job resolves (a `404` under RLS is still an audited op).
    let event = audit::audit_event(
        &caller,
        "vala.query.async.status",
        &format!("vala.query.async/{}", job_uid.0),
        &Permission::bifrost_query_read().to_string(),
        AuditDecision::Allow,
        AuditResult::Success,
        "async status read",
    );
    audit::append_on(&mut conn, &event).await?;
    conn.commit().await.map_err(storage_error)?;

    status.ok_or_else(|| WyrdError::NotFound {
        message: "async query job not found".to_owned(),
        details: json!({ "job_uid": job_uid.0 }),
    })
}

/// Execute a pre-built `LogicalPlan` through the typed seam (M-07).
///
/// Gates (in order):
///   1. `bifrost_query_read` — audited deny on failure.
///   2. Execution under a wall-clock budget (`SYNC_QUERY_TIMEOUT`).
///   3. Row ceiling (`limit + 1`): if more than `limit` rows are present the
///      returned `has_more` flag is `true` and the batch set is truncated to
///      `limit` rows.
///
/// Returns the collected batches (already within `limit`) plus the `has_more`
/// signal used by the caller to issue a signed page token.
pub async fn run_plan_query(
    state: &AppState,
    caller: &Caller,
    plan: datafusion::logical_expr::LogicalPlan,
    audit_op: &str,
    limit: u32,
) -> Result<(Vec<RecordBatch>, bool), WyrdError> {
    authorize_audited(
        state,
        caller,
        Permission::bifrost_query_read(),
        audit_op,
        "vala.query",
    )
    .await?;

    // A fresh tenant-scoped context runs the TenantPredicateRule analyzer on the
    // incoming plan; providers are embedded in the plan's TableScan sources.
    let ctx = if let Some(pool) = &state.bifrost_query_memory {
        wyrd_session_context_with_pool(caller.data_tenant_id, pool.clone())
            .map_err(map_datafusion_error)?
    } else if let Some(memory) = &state.bifrost_memory {
        wyrd_session_context_with_memory(caller.data_tenant_id, memory.bifrost_limit_bytes() / 4)
            .map_err(map_datafusion_error)?
    } else {
        wyrd_session_context(caller.data_tenant_id)
    };
    let limit_plus_one = (limit as usize).saturating_add(1);

    let (schema, batches) = tokio::time::timeout(floor::SYNC_QUERY_TIMEOUT, async {
        let df = ctx.execute_logical_plan(plan).await?;
        let df = df.limit(0, Some(limit_plus_one))?;
        let schema = df.schema().as_arrow().clone();
        let batches = df.collect().await?;
        Ok::<_, datafusion::error::DataFusionError>((schema, batches))
    })
    .await
    .map_err(|_| floor::query_timeout())?
    .map_err(map_datafusion_error)?;

    // Count total rows across all batches.
    let total_rows: usize = batches.iter().map(RecordBatch::num_rows).sum();
    let has_more = total_rows > limit as usize;

    // Truncate to exactly `limit` rows if has_more.
    let batches = if has_more {
        truncate_batches(batches, limit as usize, &schema)
    } else {
        batches
    };

    // Audit the authorized, successful typed query.
    let event = audit::audit_event(
        caller,
        audit_op,
        "vala.query",
        &Permission::bifrost_query_read().to_string(),
        wyrd_spec::vala::api::AuditDecision::Allow,
        wyrd_spec::vala::api::AuditResult::Success,
        "typed plan query executed",
    );
    audit::record_audit(state.postgres.vala_pool(), caller.data_tenant_id, &event).await?;

    Ok((batches, has_more))
}

/// Truncate a batch list to at most `limit` rows, re-slicing the last batch
/// if needed rather than collecting row-by-row.
fn truncate_batches(
    batches: Vec<RecordBatch>,
    limit: usize,
    _schema: &arrow::datatypes::Schema,
) -> Vec<RecordBatch> {
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
mod pg_tests {
    use super::*;
    use std::time::Duration;

    use wyrd_runtime::{PermissionSet, Principal, PrincipalId, PrincipalKind};
    use wyrd_spec::ids::DataTenantId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::AsyncJobState;
    use wyrd_storage::{BackendConfig, StorageHandle, StorageSettings};

    /// True when a tenant-scoped query audit row for `request_id` and `decision`
    /// exists.
    async fn has_audit(
        pool: &sqlx::PgPool,
        tenant: DataTenantId,
        request_id: &str,
        decision: &str,
    ) -> bool {
        let mut conn = TenantConn::acquire(pool, tenant)
            .await
            .expect("tenant conn");
        let rows = vala_sql::queries::audit_outbox::list_audit_events_for_resource(
            &mut conn,
            "vala.query",
            0,
            10_000,
        )
        .await
        .expect("list audit rows");
        conn.commit().await.expect("commit");
        rows.iter()
            .any(|r| r.request_id == request_id && r.decision == decision)
    }

    /// Build an `AppState` whose `pool` is the shared fixture's `wyrd_app` pool so
    /// async-path tests persist rows under RLS.
    async fn db_state() -> AppState {
        let root = tempfile::tempdir().expect("temp dir");
        let storage = StorageHandle::from_settings(StorageSettings {
            backend: BackendConfig::Local {
                root: root.path().to_path_buf(),
            },
            require_encryption: false,
            presign_ttl: Duration::from_secs(600),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
            public_base_url: Some("https://wyrd.test".to_owned()),
        })
        .await
        .expect("local storage handle");
        let pool = crate::test_support::test_pool().await;
        let wyrd = wyrd_sql::WyrdPostgres::from_pools(pool.clone(), None);
        let vala = vala_sql::ValaPostgres::from_pool(pool);
        let postgres = Arc::new(crate::postgres::ServerPostgres::from_parts(wyrd, vala));
        AppState::new(
            postgres,
            Arc::clone(&storage),
            crate::test_support::test_catalog().await,
        )
    }

    async fn caller_with(permissions: impl IntoIterator<Item = Permission>) -> Caller {
        let tenant = crate::test_support::test_tenant().await;
        Caller {
            data_tenant_id: tenant,
            principal: Principal::new(
                PrincipalId::new(uuid::Uuid::now_v7()),
                PrincipalKind::User,
                tenant,
                vec![],
                PermissionSet::from_iter(permissions),
            ),
            request_id: RequestId::parse(&uuid::Uuid::now_v7().to_string())
                .expect("request id parses"),
        }
    }

    fn async_req(sql: &str) -> AsyncQueryRequest {
        AsyncQueryRequest {
            sql: sql.to_owned(),
            params: vec![],
        }
    }

    // These tests drive the shared embedded-Postgres pool; they run on the
    // process-wide persistent runtime (never a per-test `#[tokio::test]` runtime)
    // so the shared pool is not poisoned by a runtime that dies at test end.
    // See `crate::test_support`.

    #[test]
    fn query_sync_requires_read_permission() {
        wyrd_runtime::runtime().block_on(async {
            let state = db_state().await;
            let caller = caller_with([]).await;
            let err = run_sync_query(
                &state,
                caller,
                SyncQueryRequest {
                    sql: "SELECT 1".to_owned(),
                    params: vec![],
                },
            )
            .await
            .expect_err("no read permission is denied");
            assert_eq!(err.status(), 403);
        });
    }

    #[test]
    fn query_sync_executes_select_and_reports_headers() {
        wyrd_runtime::runtime().block_on(async {
            let state = db_state().await;
            let caller = caller_with([Permission::bifrost_query_read()]).await;
            let result = run_sync_query(
                &state,
                caller,
                SyncQueryRequest {
                    sql: "SELECT 1 AS n".to_owned(),
                    params: vec![],
                },
            )
            .await
            .expect("tableless SELECT executes");
            assert_eq!(result.row_count, 1);
            assert!(!result.body.is_empty(), "IPC body is non-empty");
            assert!(
                !result.schema_fingerprint.is_empty(),
                "schema fingerprint is set"
            );
        });
    }

    #[test]
    fn query_async_requires_read_permission() {
        wyrd_runtime::runtime().block_on(async {
            let state = db_state().await;
            let caller = caller_with([]).await;
            let err = submit_async_query(&state, caller, async_req("SELECT 1"))
                .await
                .expect_err("no read permission is denied");
            assert_eq!(err.status(), 403);
        });
    }

    #[test]
    fn query_async_rejects_invalid_sql_before_enqueue() {
        wyrd_runtime::runtime().block_on(async {
            let state = db_state().await;
            let caller = caller_with([Permission::bifrost_query_read()]).await;
            let err =
                submit_async_query(&state, caller, async_req("DROP TABLE \"vala.bifrost.t\""))
                    .await
                    .expect_err("non-SELECT is rejected before enqueue");
            assert_eq!(err.status(), 400);
            assert_eq!(err.code(), "WYRD_VALA_400_QUERY_INVALID_SQL");
        });
    }

    #[test]
    fn query_async_enqueues_queued_job_idempotently_and_status_roundtrips() {
        wyrd_runtime::runtime().block_on(async {
            let state = db_state().await;
            let caller = caller_with([Permission::bifrost_query_read()]).await;
            // Unique SQL per run so the idempotency key does not collide with a
            // prior test-process row in the shared fixture.
            let sql = format!("SELECT {} AS n", uuid::Uuid::now_v7().as_u128());

            let first = submit_async_query(&state, caller.clone(), async_req(&sql))
                .await
                .expect("valid submission enqueues");
            assert_eq!(first.state, AsyncJobState::Queued);
            assert_eq!(
                first.executor_availability,
                ExecutorAvailability::PendingStage5
            );

            let second = submit_async_query(&state, caller.clone(), async_req(&sql))
                .await
                .expect("retry is idempotent");
            assert_eq!(
                first.job_uid, second.job_uid,
                "idempotent retry returns the same job"
            );

            let status = get_async_query_status(&state, caller, first.job_uid)
                .await
                .expect("status resolves for the enqueued job");
            assert_eq!(status.job_uid, first.job_uid);
            assert_eq!(status.state, AsyncJobState::Queued);
            assert_eq!(
                status.executor_availability,
                ExecutorAvailability::PendingStage5
            );
        });
    }

    #[test]
    fn query_async_status_unknown_job_is_not_found() {
        wyrd_runtime::runtime().block_on(async {
            let state = db_state().await;
            let caller = caller_with([Permission::bifrost_query_read()]).await;
            let err = get_async_query_status(&state, caller, JobUid(uuid::Uuid::now_v7()))
                .await
                .expect_err("unknown job is not found");
            assert_eq!(err.status(), 404);
        });
    }

    #[test]
    fn query_async_submit_appends_allow_audit_row() {
        wyrd_runtime::runtime().block_on(async {
            let state = db_state().await;
            let caller = caller_with([Permission::bifrost_query_read()]).await;
            let tenant = caller.data_tenant_id;
            let request_id = caller.request_id.as_str().to_owned();
            let sql = format!("SELECT {} AS n", uuid::Uuid::now_v7().as_u128());

            submit_async_query(&state, caller, async_req(&sql))
                .await
                .expect("valid submission enqueues");

            assert!(
                has_audit(state.postgres.vala_pool(), tenant, &request_id, "allow").await,
                "a successful async submit appends one allow audit row"
            );
        });
    }

    #[test]
    fn query_sync_denied_appends_deny_audit_row() {
        wyrd_runtime::runtime().block_on(async {
            let state = db_state().await;
            let caller = caller_with([]).await;
            let tenant = caller.data_tenant_id;
            let request_id = caller.request_id.as_str().to_owned();

            let err = run_sync_query(
                &state,
                caller,
                SyncQueryRequest {
                    sql: "SELECT 1".to_owned(),
                    params: vec![],
                },
            )
            .await
            .expect_err("no read permission is denied");
            assert_eq!(err.status(), 403);

            assert!(
                has_audit(state.postgres.vala_pool(), tenant, &request_id, "deny").await,
                "an RBAC denial appends one deny audit row in its own tx"
            );
        });
    }
}
