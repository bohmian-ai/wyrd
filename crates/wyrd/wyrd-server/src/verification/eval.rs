//! The continuous Eval arm: one committed observation through the existing
//! Vala Eval engine.
//!
//! [`EvalEngine`] reads the run's exact record from its frozen UTC-day
//! partition, samples it, waits for its trace when a trace task needs one, and
//! scores it through [`ScenarioScoring::score_record`] with the production
//! [`SkaldJudgeInvoker`]. It owns no lifecycle: every outcome is an
//! [`EngineOutcome`] the runner settles. An execution error is always a retry
//! and never a failed assertion; a trace that has not landed waits, and an
//! input read Bifrost refuses at admission defers without spending an attempt.

use std::sync::Arc;
use std::time::Duration;

use arrow::array::{Array, AsArray as _, BinaryArray, Int32Array, Int64Array, StringArray};
use arrow::datatypes::DataType;
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use base64::Engine as _;
use chrono::{DateTime, Utc};
use serde_json::{Map, Value, json};
use skald_runtime::ProviderRegistry;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::oracle::AuthorizedQueryContext;
use vala_eval::orchestrator::{
    AgentCardResolver, MediaResolver, PromptCardResolver, ScenarioScoring, SkaldJudgeInvoker,
};
use vala_eval::sampling::RecordSample;
use vala_eval::{EvalReport, InMemoryTraceSource, JudgeError};
use vala_sql::queries::olap_catalog::get_by_fqn;
use wyrd_runtime::permission::PermissionSet;
use wyrd_runtime::principal::{Principal, PrincipalId, PrincipalKind};
use wyrd_runtime::{
    Action, BifrostPermissionScope, BifrostTableScope, Permission, PermissionScope,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::card::agent::AgentSpec;
use wyrd_spec::card::eval::EvalSpec;
use wyrd_spec::envelope::Spec;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::VerificationRunId;
use wyrd_spec::reference::CardRef;
use wyrd_spec::reference::CardRefScope;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::storage::StorageBackendKind;
use wyrd_spec::vala::api::{AuthMethod, BifrostQueryRequest};
use wyrd_spec::vala::error::BifrostError;
use wyrd_spec::vala::eval::media::MediaRef as EvalMediaRef;
use wyrd_spec::vala::eval::record::EvalRecordObservation;
use wyrd_spec::vala::eval::{EvalSampling, EvalTask};
use wyrd_spec::vala::ids::{RunId, SpanId, TraceId};
use wyrd_spec::vala::trace::{
    InstrumentationScope, Resource, SpanEvent, SpanKind, SpanLink, SpanRecord, SpanStatus,
};
use wyrd_spec::verification::{VerificationError, VerificationVerdict};
use wyrd_sql::queries::auth::system_principal_id;
use wyrd_sql::queries::cards::{get_card_by_ref, get_card_by_uid};
use wyrd_sql::queries::verifier_runs::{ClaimedRun, RunInput, TerminalStatus};
use wyrd_storage::tenant_path;
use wyrd_storage::{StorageError, StorageHandle};
use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValueList, any_value};
use wyrd_tonic::prost::Message as _;

use super::engines::{EngineOutcome, VerifierReport};
use crate::query::scheduled::ScheduledQueryCaller;
use crate::state::AppState;

/// Stable error code when the run's record cannot be read or decoded.
pub const RECORD_UNAVAILABLE: &str = "eval_record_unavailable";
/// Stable error code when the record's trace cannot be read.
pub const TRACE_UNAVAILABLE: &str = "eval_trace_unavailable";
/// Stable error code while the record's trace has not landed.
pub const AWAITING_TRACE: &str = "eval_awaiting_trace";
/// Stable error code for a sampling, judge, media, or task execution failure.
pub const EXECUTION_FAILED: &str = "eval_execution_failed";
/// Stable error code for an Eval spec the engine cannot plan.
pub const SPEC_INVALID: &str = "eval_spec_invalid";

/// Largest media object one judge call may read.
// ponytail: fixed ceiling; make it a runtime limit when an operator needs to tune it.
const MEDIA_LIMIT_BYTES: u64 = 20 * 1024 * 1024;

/// Most spans one Eval trace read decodes; a trace with more is a
/// trace-source failure, refused before any task or provider runs.
// ponytail: fixed server-owned ceiling; make it a runtime limit when an
// operator needs to tune it.
pub const TRACE_SPAN_LIMIT: usize = 10_000;

/// Deadline of one Bifrost read issued by the engine.
const READ_DEADLINE_MS: u64 = 30_000;
/// Stable query error of a tenant span table no export has created yet.
const SPANS_NOT_CREATED: &str = "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND";

/// Owner of continuous Eval execution over the server's own state.
#[derive(Clone)]
pub struct EvalEngine {
    /// Server state supplying Bifrost reads, the registry, and object storage.
    state: AppState,
    /// Model providers the production judge invoker calls.
    providers: Arc<ProviderRegistry>,
    /// How long after its record a run may still read trace spans: the
    /// runtime's trace deadline plus one day of slack for enqueue latency and
    /// clock skew. It closes the upper end of every span read.
    trace_window: chrono::Duration,
}

impl EvalEngine {
    /// Build the engine over `state`, judging through `providers`, reading
    /// traces up to `trace_deadline` (plus one day of slack) after a record.
    #[must_use]
    pub fn new(
        state: AppState,
        providers: Arc<ProviderRegistry>,
        trace_deadline: Duration,
    ) -> Self {
        let trace_deadline = chrono::Duration::from_std(trace_deadline)
            .unwrap_or_else(|_| chrono::Duration::minutes(5));
        Self {
            state,
            providers,
            trace_window: trace_deadline + chrono::Duration::days(1),
        }
    }

    /// Execute one claimed Eval run of `tenant` under `spec`.
    ///
    /// Reads the frozen record, samples it before any trace or task work
    /// (a sampled-out record completes inconclusive with an empty report),
    /// waits for its trace when a trace task needs it, then scores it and maps
    /// the report to the common verdict. Read, sampling, and scoring failures
    /// retry, except that a read refused at Bifrost admission defers; a
    /// non-Eval input or an unplannable spec terminates `errored`.
    pub async fn execute(
        &self,
        tenant: DataTenantId,
        run: &ClaimedRun,
        spec: &EvalSpec,
    ) -> EngineOutcome {
        let RunInput::EvalRecord {
            record_id,
            event_time,
        } = &run.input
        else {
            return terminal(failure(
                SPEC_INVALID,
                "an Eval run requires an Eval record input",
            ));
        };
        let run_id = run.lease.run_id;
        let reader = match BifrostReader::new(&self.state, tenant).await {
            Ok(reader) => reader,
            Err(error) => {
                return EngineOutcome::Retry(failed(
                    run_id,
                    RECORD_UNAVAILABLE,
                    "the Eval read authority is unavailable",
                    &error,
                ));
            }
        };
        let record = match reader
            .record(&run.subject_card_uid.to_string(), record_id, *event_time)
            .await
        {
            Ok(record) => record,
            Err(error) => {
                return error.outcome(run_id, RECORD_UNAVAILABLE, "the Eval record cannot be read");
            }
        };
        match sampled(run, spec, record_id, &record.context) {
            Ok(true) => {}
            Ok(false) => {
                return EngineOutcome::Completed(VerifierReport::Eval {
                    report: EvalReport::default(),
                    verdict: VerificationVerdict::Inconclusive,
                });
            }
            Err(error) => {
                return EngineOutcome::Retry(failed(
                    run_id,
                    EXECUTION_FAILED,
                    "the Eval record cannot be sampled",
                    &error,
                ));
            }
        }
        let traces = InMemoryTraceSource::new();
        if let Some(trace_id) = record.trace_id.filter(|_| needs_trace(spec)) {
            match reader.spans(trace_id, *event_time, self.trace_window).await {
                Ok(spans) if spans.is_empty() => {
                    return EngineOutcome::AwaitingTrace(failure(
                        AWAITING_TRACE,
                        &format!("trace {trace_id} has not landed"),
                    ));
                }
                Ok(spans) => {
                    traces.insert(trace_id, spans).await;
                }
                Err(error) => {
                    return error.outcome(
                        run_id,
                        TRACE_UNAVAILABLE,
                        "the record's trace cannot be read",
                    );
                }
            }
        }
        self.score(tenant, run, spec, &record, traces).await
    }

    /// Score `record` through the one Eval execution path and map its report.
    ///
    /// A trace that is still missing when a task needs it waits; every other
    /// execution error retries and never becomes a failed assertion.
    async fn score(
        &self,
        tenant: DataTenantId,
        run: &ClaimedRun,
        spec: &EvalSpec,
        record: &EvalRecordObservation,
        traces: InMemoryTraceSource,
    ) -> EngineOutcome {
        let registry = Arc::new(TenantRegistry {
            state: self.state.clone(),
            tenant,
        });
        let media = Arc::new(TenantMedia {
            storage: Arc::clone(&self.state.storage),
            tenant,
        });
        let judge = SkaldJudgeInvoker::new(
            Arc::clone(&self.providers),
            Arc::clone(&registry) as Arc<dyn AgentCardResolver>,
            registry,
        )
        .with_media_resolver(media);
        let scoring = match ScenarioScoring::new(
            Arc::new(spec.clone()),
            Arc::new(judge),
            Arc::new(traces),
            Duration::from_millis(READ_DEADLINE_MS),
        ) {
            Ok(scoring) => scoring,
            Err(error) => {
                return terminal(failed(
                    run.lease.run_id,
                    SPEC_INVALID,
                    "the Eval spec cannot be planned",
                    &error,
                ));
            }
        };
        let eval_run = RunId::from_string(run.lease.run_id.to_string());
        match scoring.score_record(eval_run, None, record).await {
            Ok(report) => match VerifierReport::eval(report, spec) {
                Ok(report) => EngineOutcome::Completed(report),
                Err(error) => EngineOutcome::Retry(failed(
                    run.lease.run_id,
                    EXECUTION_FAILED,
                    "the Eval report cannot be captured",
                    &error,
                )),
            },
            Err(error) if error.awaits_trace() => EngineOutcome::AwaitingTrace(failure(
                AWAITING_TRACE,
                "the record's trace has not landed",
            )),
            Err(error) => EngineOutcome::Retry(failed(
                run.lease.run_id,
                EXECUTION_FAILED,
                "the Eval tasks cannot be executed",
                &error,
            )),
        }
    }
}

/// Whether any task of `spec` asserts over the record's trace.
fn needs_trace(spec: &EvalSpec) -> bool {
    spec.tasks
        .values()
        .any(|task| matches!(task, EvalTask::TraceAssertion(_)))
}

/// Whether `spec`'s sampling policy selects `record` of `run`.
///
/// `every_nth` selects on the ordinal the queue stored once at enqueue and
/// every claim returns, so a retried, reclaimed, or restarted run reaches the
/// same decision; other policies ignore it.
///
/// # Errors
/// Returns a sampling failure, such as a missing `deterministic_by_hash` key,
/// or an `every_nth` run that carries no stored ordinal.
fn sampled(
    run: &ClaimedRun,
    spec: &EvalSpec,
    record_id: &str,
    context: &Value,
) -> Result<bool, String> {
    let ordinal = if matches!(spec.sampling, Some(EvalSampling::EveryNth { .. })) {
        run.observation_ordinal
            .ok_or("an every_nth run has no stored observation ordinal")?
    } else {
        0
    };
    RecordSample {
        record_id,
        context,
        ordinal,
    }
    .selected(spec.sampling.as_ref())
    .map_err(|error| error.to_string())
}

/// A terminal `errored` outcome carrying `error`.
fn terminal(error: VerificationError) -> EngineOutcome {
    EngineOutcome::Terminal(TerminalStatus::Errored, error)
}

/// A run failure with a stable `code` and fixed, non-sensitive public
/// `message`.
///
/// The runner persists this error and run status exposes it, so `message`
/// names only the operation that failed; it never carries dependency text.
fn failure(code: &str, message: &str) -> VerificationError {
    VerificationError {
        code: code.to_owned(),
        message: message.to_owned(),
    }
}

/// The public failure of `step` under `code`, with its dependency `cause`
/// kept to the protected diagnostic log.
///
/// SQL, Bifrost, registry, and provider errors can carry query text, private
/// storage detail, or provider response bodies. They are written once, as a
/// structured `warn` event correlated by `run_id` and `code`, and the
/// returned error carries only `code` and the fixed `step` text. Causes that
/// may name a private storage locator must already be redacted by their
/// producer (see [`TenantMedia`]).
fn failed(
    run_id: VerificationRunId,
    code: &str,
    step: &'static str,
    cause: &dyn std::fmt::Display,
) -> VerificationError {
    tracing::warn!(%run_id, code, step, %cause, "continuous Eval step failed");
    failure(code, step)
}

/// Why one Eval input read produced no input.
#[derive(Debug)]
enum ReadError {
    /// Bifrost refused the read at query admission: shared query capacity is
    /// saturated, which says nothing about this run's input.
    Admission,
    /// The read failed, or its rows are absent or do not decode.
    Failed(String),
}

impl ReadError {
    /// The engine outcome of this read failure of run `run_id` under the
    /// stable `code`, publicly described by the fixed `step` text.
    ///
    /// Admission refusal defers the run without spending an attempt; every
    /// other failure is an ordinary retry whose cause is only logged.
    fn outcome(self, run_id: VerificationRunId, code: &str, step: &'static str) -> EngineOutcome {
        match self {
            Self::Admission => EngineOutcome::Deferred(failure(
                code,
                "Bifrost refused the input read at query admission",
            )),
            Self::Failed(cause) => EngineOutcome::Retry(failed(run_id, code, step, &cause)),
        }
    }
}

impl From<String> for ReadError {
    /// A read or decode failure described by `message`.
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}

impl From<&str> for ReadError {
    /// A read or decode failure described by `message`.
    fn from(message: &str) -> Self {
        Self::Failed(message.to_owned())
    }
}

impl From<WyrdError> for ReadError {
    /// Classify a query error: admission refusal is backpressure, anything
    /// else is a failed read.
    fn from(error: WyrdError) -> Self {
        match error {
            WyrdError::Vala {
                error: BifrostError::QueryAdmissionRejected,
            } => Self::Admission,
            error => Self::Failed(error.to_string()),
        }
    }
}

/// Logical schema and table of each tenant table continuous Eval reads its
/// inputs from: committed observations and their trace spans. The System read
/// authority reaches exactly these tables and nothing else.
const EVAL_INPUT_TABLES: [(&str, &str); 2] = [("eval", "observations"), ("traces", "spans")];

/// Why the tenant's System principal cannot be authorized to read Eval inputs.
#[derive(Debug, thiserror::Error)]
pub enum EvalReadAuthorityError {
    /// The tenant has no active System principal with a `UUIDv7` id.
    #[error("the tenant has no active System principal")]
    SystemPrincipalMissing,
    /// Reading the System principal or the input table identities failed.
    #[error("the Eval read authority is unavailable: {0}")]
    Unavailable(String),
    /// The authorized context's tenant invariant does not hold.
    #[error(transparent)]
    Context(#[from] BifrostError),
}

/// The tenant's persisted System principal, authorized by the server to read
/// exactly continuous Eval's input tables.
///
/// This is the same credentialless, role-free, per-tenant principal whose
/// exact-Verifier `bifrost_record:write` token publishes verification results,
/// with a separate server-minted read authority: one `bifrost_query:read`
/// grant per existing [`EVAL_INPUT_TABLES`] table, scoped to that table's
/// registered UID. It never carries a general query grant, never names a
/// Verifier write scope, and is never issued as a token; Oracle authorizes and
/// audits every read under it like any caller's.
#[derive(Debug, Clone)]
pub struct EvalReadAuthority {
    /// The System principal's authorized query context for one tenant.
    context: AuthorizedQueryContext,
}

impl EvalReadAuthority {
    /// Resolve `tenant`'s System principal and mint its Eval input read scope.
    ///
    /// Reads the stored System principal id under the tenant's RLS bind, then
    /// the registered UID of each Eval input table the tenant already has; a
    /// table not yet created (a tenant's span table appears with its first
    /// export) gets no grant, so a read of it stays not-found. Nothing is
    /// written, so both transactions end without commit.
    ///
    /// # Errors
    /// Returns [`EvalReadAuthorityError::SystemPrincipalMissing`] when the
    /// tenant has no active `UUIDv7` System principal,
    /// [`EvalReadAuthorityError::Unavailable`] when a control-plane read fails
    /// or a stored table UID is malformed, and
    /// [`EvalReadAuthorityError::Context`] when the context's tenant invariant
    /// fails.
    pub async fn resolve(
        state: &AppState,
        tenant: DataTenantId,
    ) -> Result<Self, EvalReadAuthorityError> {
        let unavailable =
            |error: &dyn std::fmt::Display| EvalReadAuthorityError::Unavailable(error.to_string());
        let mut conn = state
            .postgres
            .tenant_conn(tenant)
            .await
            .map_err(|error| unavailable(&error))?;
        let id = system_principal_id(&mut conn)
            .await
            .map_err(|error| unavailable(&error))?
            .filter(|id| id.get_version_num() == 7)
            .ok_or(EvalReadAuthorityError::SystemPrincipalMissing)?;
        drop(conn);
        let mut conn = state
            .postgres
            .vala()
            .tenant_conn(tenant)
            .await
            .map_err(|error| unavailable(&error))?;
        let mut grants = Vec::with_capacity(EVAL_INPUT_TABLES.len());
        for (schema, table) in EVAL_INPUT_TABLES {
            let Some(row) = get_by_fqn(&mut conn, &format!("vala.{schema}.{table}"))
                .await
                .map_err(|error| unavailable(&error))?
            else {
                continue;
            };
            let table_uid =
                uuid::Uuid::from_slice(&row.table_uid).map_err(|error| unavailable(&error))?;
            grants.push(Permission {
                resource: wyrd_runtime::Resource::BifrostQuery,
                action: Action::Read,
                scope: PermissionScope::Bifrost(BifrostPermissionScope::Table(BifrostTableScope {
                    catalog: "vala".to_owned(),
                    schema: schema.to_owned(),
                    table_uid,
                })),
            });
        }
        let principal = Principal::new(
            PrincipalId::new(id),
            PrincipalKind::System {
                card_ref_scope: CardRefScope::default(),
            },
            tenant,
            Vec::new(),
            PermissionSet::from_iter(grants),
        );
        let context = AuthorizedQueryContext::try_new(
            principal,
            tenant,
            RequestId::now_v7(),
            None,
            AuthMethod::Internal,
            Permission::bifrost_query_read(),
        )?;
        Ok(Self { context })
    }

    /// The System principal's authorized query context.
    #[must_use]
    pub const fn context(&self) -> &AuthorizedQueryContext {
        &self.context
    }
}

/// Tenant-scoped Bifrost reads of Eval inputs through the ordinary query entry.
struct BifrostReader {
    /// Caller bound to the tenant System principal's Eval input read authority.
    caller: ScheduledQueryCaller,
}

impl BifrostReader {
    /// Bind a reader for `tenant` to its [`EvalReadAuthority`].
    ///
    /// # Errors
    /// Returns every [`EvalReadAuthority::resolve`] failure; no read is
    /// attempted without the System principal's authority.
    async fn new(state: &AppState, tenant: DataTenantId) -> Result<Self, EvalReadAuthorityError> {
        let authority = EvalReadAuthority::resolve(state, tenant).await?;
        Ok(Self {
            caller: ScheduledQueryCaller::new(
                state.clone(),
                authority.context,
                CancellationToken::new(),
            ),
        })
    }

    /// Run `sql` over committed and live rows and return its batches.
    ///
    /// # Errors
    /// Returns the query failure.
    #[tracing::instrument(name = "verification.evidence_read", skip_all)]
    async fn query(&self, sql: String) -> Result<Vec<RecordBatch>, WyrdError> {
        let mut batches = Vec::new();
        self.caller
            .run_with(
                BifrostQueryRequest {
                    sql,
                    deadline_ms: i64::try_from(READ_DEADLINE_MS).ok(),
                },
                |batch| {
                    batches.push(batch);
                    Ok(())
                },
            )
            .await?;
        Ok(batches)
    }

    /// Read one committed observation of `subject` by its record ID, pruned
    /// to the UTC day of its frozen server `event_time`.
    ///
    /// # Errors
    /// Returns [`ReadError::Admission`] when Bifrost refuses the read at
    /// admission, and [`ReadError::Failed`] for any other query failure or
    /// when the record is absent or does not decode.
    async fn record(
        &self,
        subject: &str,
        record_id: &str,
        event_time: DateTime<Utc>,
    ) -> Result<EvalRecordObservation, ReadError> {
        let (start, end) = utc_day(event_time);
        let batches = self
            .query(format!(
                "SELECT record_id, session_id, context, trace_id, span_id, created_at, media \
                 FROM vala.eval.observations \
                 WHERE card_uid = '{}' AND record_id = '{}' \
                   AND wyrd_event_time >= TIMESTAMP '{start}' \
                   AND wyrd_event_time < TIMESTAMP '{end}' \
                 LIMIT 1",
                quoted(subject),
                quoted(record_id),
            ))
            .await?;
        let batch = batches
            .iter()
            .find(|batch| batch.num_rows() > 0)
            .ok_or_else(|| format!("record {record_id} is not visible"))?;
        let trace_id = bytes(batch, "trace_id", 0)?
            .map(|id| {
                id.try_into()
                    .map_err(|_| "trace_id is not 16 bytes".to_owned())
            })
            .transpose()?
            .map(TraceId::from_bytes)
            .transpose()
            .map_err(|error| error.to_string())?;
        let span_id = bytes(batch, "span_id", 0)?
            .map(|id| {
                id.try_into()
                    .map_err(|_| "span_id is not 8 bytes".to_owned())
            })
            .transpose()?
            .map(SpanId::from_bytes)
            .transpose()
            .map_err(|error| error.to_string())?;
        let created_at = int64(batch, "created_at", 0)?
            .and_then(DateTime::<Utc>::from_timestamp_micros)
            .ok_or("created_at is missing")?;
        let json_text = |name| -> Result<Value, String> {
            text(batch, name, 0)?
                .map(|raw| serde_json::from_str(&raw).map_err(|error| error.to_string()))
                .transpose()
                .map(Option::unwrap_or_default)
        };
        serde_json::from_value(json!({
            "record_id": text(batch, "record_id", 0)?,
            "session_id": text(batch, "session_id", 0)?,
            "context": json_text("context")?,
            "trace_id": trace_id.map(|id| id.to_hex()),
            "span_id": span_id.map(|id| id.to_hex()),
            "created_at": created_at,
            "media": json_text("media")?,
        }))
        .map_err(|error| format!("record {record_id} does not decode: {error}").into())
    }

    /// Read the visible spans of `trace_id` received from the start of the
    /// day before the record's day to the end of the day containing
    /// `event_time + window`, in start-time then span-ID order.
    ///
    /// The one total order makes positional selectors such as `$.spans[0]`
    /// reproducible across retries and restarts. At most
    /// [`TRACE_SPAN_LIMIT`] spans are decoded: the query asks for one more,
    /// and receiving it fails the read before any row is decoded.
    ///
    /// # Errors
    /// Returns [`ReadError::Admission`] when Bifrost refuses the read at
    /// admission, and [`ReadError::Failed`] for any other query or decode
    /// failure, a window past the representable range, or a trace over
    /// [`TRACE_SPAN_LIMIT`] spans.
    async fn spans(
        &self,
        trace_id: TraceId,
        event_time: DateTime<Utc>,
        window: chrono::Duration,
    ) -> Result<Vec<SpanRecord>, ReadError> {
        let (start, _) = utc_day(event_time - chrono::Duration::days(1));
        let (_, end) = utc_day(
            event_time
                .checked_add_signed(window)
                .ok_or("the trace window is out of range")?,
        );
        let batches = match self
            .query(format!(
                "SELECT span_id, parent_span_id, trace_state, flags, name, kind, \
                        start_time_unix_nano, end_time_unix_nano, status_code, status_message, \
                        attributes, dropped_attributes_count, events, dropped_events_count, \
                        links, dropped_links_count, scope_name, service_name \
                 FROM vala.traces.spans \
                 WHERE trace_id = X'{}' \
                   AND wyrd_event_time >= TIMESTAMP '{start}' \
                   AND wyrd_event_time < TIMESTAMP '{end}' \
                 ORDER BY start_time_unix_nano, span_id \
                 LIMIT {}",
                trace_id.to_hex(),
                TRACE_SPAN_LIMIT + 1,
            ))
            .await
        {
            Ok(batches) => batches,
            // A tenant's span table is created by its first export, so its
            // absence means no span of this trace has landed yet.
            Err(error) if error.code() == SPANS_NOT_CREATED => Vec::new(),
            Err(error) => return Err(error.into()),
        };
        let rows: usize = batches.iter().map(RecordBatch::num_rows).sum();
        if rows > TRACE_SPAN_LIMIT {
            return Err(format!("trace {trace_id} exceeds {TRACE_SPAN_LIMIT} spans").into());
        }
        let mut spans = Vec::with_capacity(rows);
        for batch in &batches {
            for row in 0..batch.num_rows() {
                spans.push(span(batch, row, trace_id)?);
            }
        }
        Ok(spans)
    }
}

/// Decode one `vala.traces.spans` row into the engine's span shape,
/// including its persisted events, links, and dropped counts.
///
/// # Errors
/// Returns a description of a missing or malformed column.
// ponytail: resource attributes and scope version are not projected; decode
// them when a trace assertion needs to select them.
fn span(batch: &RecordBatch, row: usize, trace_id: TraceId) -> Result<SpanRecord, String> {
    let span_id = |name| -> Result<Option<SpanId>, String> {
        bytes(batch, name, row)?
            .map(|id| {
                <[u8; 8]>::try_from(id)
                    .map_err(|_| format!("{name} is not 8 bytes"))
                    .and_then(|id| SpanId::from_bytes(id).map_err(|error| error.to_string()))
            })
            .transpose()
    };
    let nanos = |name| -> Result<DateTime<Utc>, String> {
        let value = int64(batch, name, row)?.ok_or_else(|| format!("{name} is missing"))?;
        Ok(DateTime::<Utc>::from_timestamp_nanos(value))
    };
    let (start_time, end_time) = (nanos("start_time_unix_nano")?, nanos("end_time_unix_nano")?);
    let status = match int32(batch, "status_code", row)? {
        Some(1) => SpanStatus::Ok,
        Some(2) => SpanStatus::Error {
            description: text(batch, "status_message", row)?,
        },
        _ => SpanStatus::Unset,
    };
    let kind = match int32(batch, "kind", row)? {
        Some(2) => SpanKind::Server,
        Some(3) => SpanKind::Client,
        Some(4) => SpanKind::Producer,
        Some(5) => SpanKind::Consumer,
        _ => SpanKind::Internal,
    };
    Ok(SpanRecord {
        trace_id,
        span_id: span_id("span_id")?.ok_or("span_id is missing")?,
        parent_span_id: span_id("parent_span_id")?,
        flags: u32::try_from(int64(batch, "flags", row)?.unwrap_or_default()).unwrap_or_default(),
        trace_state: text(batch, "trace_state", row)?.unwrap_or_default(),
        name: text(batch, "name", row)?.unwrap_or_default(),
        kind,
        start_time,
        end_time,
        duration_ms: u64::try_from((end_time - start_time).num_milliseconds()).unwrap_or(0),
        status,
        attributes: attribute_column(batch, "attributes", row)?,
        dropped_attributes_count: count(batch, "dropped_attributes_count", row)?,
        events: nested(batch, "events", row, event)?,
        dropped_events_count: count(batch, "dropped_events_count", row)?,
        links: nested(batch, "links", row, link)?,
        dropped_links_count: count(batch, "dropped_links_count", row)?,
        scope: InstrumentationScope {
            name: text(batch, "scope_name", row)?.unwrap_or_default(),
            version: None,
            attributes: Map::new(),
        },
        resource: Resource {
            service_name: text(batch, "service_name", row)?.unwrap_or_default(),
            service_namespace: None,
            service_version: None,
            service_instance_id: None,
            attributes: Map::new(),
        },
    })
}

/// Decode one persisted span event from row `row` of its element batch.
///
/// # Errors
/// Returns a description of a missing or malformed field.
fn event(items: &RecordBatch, row: usize) -> Result<SpanEvent, String> {
    Ok(SpanEvent {
        timestamp: DateTime::<Utc>::from_timestamp_nanos(
            int64(items, "time_unix_nano", row)?.ok_or("event time_unix_nano is missing")?,
        ),
        name: text(items, "name", row)?.unwrap_or_default(),
        attributes: attribute_column(items, "attributes", row)?,
        dropped_attributes_count: count(items, "dropped_attributes_count", row)?,
    })
}

/// Decode one persisted span link from row `row` of its element batch.
///
/// # Errors
/// Returns a description of a missing or malformed field or identifier.
fn link(items: &RecordBatch, row: usize) -> Result<SpanLink, String> {
    let trace_id = bytes(items, "trace_id", row)?.ok_or("link trace_id is missing")?;
    let span_id = bytes(items, "span_id", row)?.ok_or("link span_id is missing")?;
    Ok(SpanLink {
        trace_id: <[u8; 16]>::try_from(trace_id)
            .map_err(|_| "link trace_id is not 16 bytes".to_owned())
            .and_then(|id| TraceId::from_bytes(id).map_err(|error| error.to_string()))?,
        span_id: <[u8; 8]>::try_from(span_id)
            .map_err(|_| "link span_id is not 8 bytes".to_owned())
            .and_then(|id| SpanId::from_bytes(id).map_err(|error| error.to_string()))?,
        trace_state: text(items, "trace_state", row)?.unwrap_or_default(),
        flags: count(items, "flags", row)?,
        attributes: attribute_column(items, "attributes", row)?,
        dropped_attributes_count: count(items, "dropped_attributes_count", row)?,
    })
}

/// Decode every element of list column `name` at `row` with `decode`; a
/// null list is empty.
///
/// The list's struct elements are viewed as one batch so the scalar column
/// readers decode their fields.
///
/// # Errors
/// Returns a description when the column is absent, is not a list of
/// non-null structs, or an element does not decode.
fn nested<T>(
    batch: &RecordBatch,
    name: &str,
    row: usize,
    decode: fn(&RecordBatch, usize) -> Result<T, String>,
) -> Result<Vec<T>, String> {
    let list = batch
        .column_by_name(name)
        .ok_or_else(|| format!("column {name} is missing"))?
        .as_list_opt::<i32>()
        .ok_or_else(|| format!("column {name} is not a list"))?;
    if list.is_null(row) {
        return Ok(Vec::new());
    }
    let elements = list.value(row);
    let elements = elements
        .as_struct_opt()
        .filter(|elements| elements.null_count() == 0)
        .ok_or_else(|| format!("column {name} does not hold non-null structs"))?;
    let elements = RecordBatch::from(elements.clone());
    (0..elements.num_rows())
        .map(|element| decode(&elements, element))
        .collect()
}

/// The canonical attribute payload in column `name` at `row` as JSON; a null
/// payload is empty.
///
/// # Errors
/// Returns the column or protobuf decode failure.
fn attribute_column(
    batch: &RecordBatch,
    name: &str,
    row: usize,
) -> Result<Map<String, Value>, String> {
    bytes(batch, name, row)?.map_or_else(|| Ok(Map::new()), |raw| attributes(&raw))
}

/// Non-negative 32-bit count in 64-bit column `name` at `row`; null is zero.
///
/// # Errors
/// Returns the column failure or a value outside `u32`.
fn count(batch: &RecordBatch, name: &str, row: usize) -> Result<u32, String> {
    u32::try_from(int64(batch, name, row)?.unwrap_or_default())
        .map_err(|_| format!("column {name} is not a 32-bit count"))
}

/// Decode a canonical `KeyValueList` attribute payload into JSON.
///
/// # Errors
/// Returns the protobuf decode failure.
fn attributes(raw: &[u8]) -> Result<Map<String, Value>, String> {
    Ok(KeyValueList::decode(raw)
        .map_err(|error| error.to_string())?
        .values
        .into_iter()
        .map(|entry| (entry.key, any_json(entry.value)))
        .collect())
}

/// One OTLP `AnyValue` as JSON; bytes become base64 text.
fn any_json(value: Option<AnyValue>) -> Value {
    match value.and_then(|value| value.value) {
        None => Value::Null,
        Some(any_value::Value::StringValue(text)) => Value::String(text),
        Some(any_value::Value::BoolValue(flag)) => Value::Bool(flag),
        Some(any_value::Value::IntValue(number)) => Value::from(number),
        Some(any_value::Value::DoubleValue(number)) => Value::from(number),
        Some(any_value::Value::BytesValue(raw)) => {
            Value::String(base64::engine::general_purpose::STANDARD.encode(raw))
        }
        Some(any_value::Value::ArrayValue(array)) => Value::Array(
            array
                .values
                .into_iter()
                .map(|item| any_json(Some(item)))
                .collect(),
        ),
        Some(any_value::Value::KvlistValue(list)) => Value::Object(
            list.values
                .into_iter()
                .map(|entry| (entry.key, any_json(entry.value)))
                .collect(),
        ),
    }
}

/// The UTC day containing `at`, as `[start, end)` SQL timestamp literals.
fn utc_day(at: DateTime<Utc>) -> (String, String) {
    let start = at.date_naive().and_hms_opt(0, 0, 0).unwrap_or_default();
    let end = start + chrono::Duration::days(1);
    let format = "%Y-%m-%d %H:%M:%S";
    (
        start.format(format).to_string(),
        end.format(format).to_string(),
    )
}

/// `value` escaped for a single-quoted SQL string literal.
fn quoted(value: &str) -> String {
    value.replace('\'', "''")
}

/// Column `name` of `batch` cast to `to`.
///
/// # Errors
/// Returns a description when the column is absent or cannot be cast.
fn column(batch: &RecordBatch, name: &str, to: &DataType) -> Result<Arc<dyn Array>, String> {
    let column = batch
        .column_by_name(name)
        .ok_or_else(|| format!("column {name} is missing"))?;
    arrow::compute::cast(column, to).map_err(|error| format!("column {name}: {error}"))
}

/// Text value of column `name` at `row`, `None` when null.
///
/// # Errors
/// Returns the column failure.
fn text(batch: &RecordBatch, name: &str, row: usize) -> Result<Option<String>, String> {
    let column = column(batch, name, &DataType::Utf8)?;
    let column = column
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| format!("column {name} is not text"))?;
    Ok(column.is_valid(row).then(|| column.value(row).to_owned()))
}

/// Byte value of column `name` at `row`, `None` when null.
///
/// # Errors
/// Returns the column failure.
fn bytes(batch: &RecordBatch, name: &str, row: usize) -> Result<Option<Vec<u8>>, String> {
    let column = column(batch, name, &DataType::Binary)?;
    let column = column
        .as_any()
        .downcast_ref::<BinaryArray>()
        .ok_or_else(|| format!("column {name} is not binary"))?;
    Ok(column.is_valid(row).then(|| column.value(row).to_vec()))
}

/// 64-bit integer value of column `name` at `row`, `None` when null.
///
/// # Errors
/// Returns the column failure.
fn int64(batch: &RecordBatch, name: &str, row: usize) -> Result<Option<i64>, String> {
    let column = column(batch, name, &DataType::Int64)?;
    let column = column
        .as_any()
        .downcast_ref::<Int64Array>()
        .ok_or_else(|| format!("column {name} is not an integer"))?;
    Ok(column.is_valid(row).then(|| column.value(row)))
}

/// 32-bit integer value of column `name` at `row`, `None` when null.
///
/// # Errors
/// Returns the column failure.
fn int32(batch: &RecordBatch, name: &str, row: usize) -> Result<Option<i32>, String> {
    let column = column(batch, name, &DataType::Int32)?;
    let column = column
        .as_any()
        .downcast_ref::<Int32Array>()
        .ok_or_else(|| format!("column {name} is not an integer"))?;
    Ok(column.is_valid(row).then(|| column.value(row)))
}

/// Resolves judge Agent and Prompt Cards from the run tenant's registry.
struct TenantRegistry {
    /// Server state owning the Wyrd Postgres registry.
    state: AppState,
    /// Tenant every lookup is scoped to.
    tenant: DataTenantId,
}

impl TenantRegistry {
    /// Load the Card `reference` names, by UID when pinned.
    ///
    /// # Errors
    /// Returns [`JudgeError::Retryable`] for a connection failure and
    /// [`JudgeError::Terminal`] for an unpinned reference without a space or
    /// an absent or unparseable Card.
    async fn spec(&self, reference: &CardRef) -> Result<Spec, JudgeError> {
        let mut conn = self
            .state
            .postgres
            .wyrd()
            .tenant_conn(self.tenant)
            .await
            .map_err(|error| JudgeError::Retryable {
                reason: error.to_string(),
            })?;
        let card = match (&reference.uid, &reference.space) {
            (Some(uid), _) => get_card_by_uid(&mut conn, uid).await,
            (None, Some(space)) => {
                get_card_by_ref(
                    &mut conn,
                    reference.kind.clone(),
                    space,
                    &reference.name,
                    &reference.version,
                )
                .await
            }
            (None, None) => {
                return Err(JudgeError::Terminal {
                    reason: format!("judge reference {reference} names no space or uid"),
                });
            }
        };
        card.map(|card| card.spec)
            .map_err(|error| JudgeError::Terminal {
                reason: error.to_string(),
            })
    }
}

#[async_trait]
impl AgentCardResolver for TenantRegistry {
    /// Resolve an Agent Card of this tenant.
    async fn resolve(&self, agent_ref: &CardRef) -> Result<AgentSpec, JudgeError> {
        match self.spec(agent_ref).await? {
            Spec::Agent(spec) => Ok(spec),
            _ => Err(JudgeError::Terminal {
                reason: format!("card {agent_ref} is not an Agent"),
            }),
        }
    }
}

#[async_trait]
impl PromptCardResolver for TenantRegistry {
    /// Resolve a Prompt Card of this tenant into its runtime prompt.
    async fn resolve(&self, prompt_ref: &CardRef) -> Result<skald_prompt::Prompt, JudgeError> {
        match self.spec(prompt_ref).await? {
            Spec::Prompt(spec) => Ok(skald_prompt::Prompt::from_native(spec.prompt)),
            _ => Err(JudgeError::Terminal {
                reason: format!("card {prompt_ref} is not a Prompt"),
            }),
        }
    }
}

/// Reads record media from the run tenant's own object storage.
struct TenantMedia {
    /// The server's storage handle.
    storage: Arc<StorageHandle>,
    /// Tenant whose path prefix every object must carry.
    tenant: DataTenantId,
}

impl TenantMedia {
    /// The judge error of media binding `binding` refused for `category`.
    ///
    /// `category` is fixed text naming why, never the URI, object key, or a
    /// storage error's `Display`; only the non-sensitive binding ID and the
    /// category reach the reason and the diagnostic log, because the reason
    /// is logged as an Eval execution cause.
    fn refused(binding: &str, category: &'static str) -> JudgeError {
        tracing::warn!(binding, category, "Eval media refused");
        JudgeError::Terminal {
            reason: format!("media `{binding}`: {category}"),
        }
    }

    /// The judge error of a storage failure while reading binding `binding`.
    ///
    /// An absent object is a terminal input error; any other storage failure
    /// may be transient and retries. Storage errors name the object key and
    /// carry backend text, so only the backend kind is logged.
    fn storage_failed(&self, binding: &str, error: &StorageError) -> JudgeError {
        if matches!(error, StorageError::ObjectNotFound { .. }) {
            return Self::refused(binding, "the object does not exist");
        }
        tracing::warn!(
            binding,
            backend = ?self.storage.backend(),
            "Eval media storage read failed"
        );
        JudgeError::Retryable {
            reason: format!("media `{binding}`: the object storage read failed"),
        }
    }
}

#[async_trait]
impl MediaResolver for TenantMedia {
    /// Resolve `media` to inline base64 content.
    ///
    /// The URI must use the configured backend's scheme and name a path inside
    /// this tenant's prefix; the object must exist, fit
    /// [`MEDIA_LIMIT_BYTES`], and declare a MIME type its kind supports. The
    /// metadata size is only a fast rejection: the body read stops one byte
    /// past the limit, and a body that reaches it is refused before encoding,
    /// so no provider call receives it. The URI itself never reaches the
    /// provider, and refusals name only the binding ID and a fixed category.
    async fn resolve(&self, media: &EvalMediaRef) -> Result<skald_spec::MediaRef, JudgeError> {
        let binding = media.id.as_str();
        let refused = |category| Self::refused(binding, category);
        let mime_type = media
            .media_type
            .clone()
            .filter(|mime| supported(media.kind, mime))
            .ok_or_else(|| refused("the media type is not supported for its kind"))?;
        let uri = url::Url::parse(&media.uri).map_err(|_| refused("the URI is not a URL"))?;
        let scheme = match self.storage.backend() {
            StorageBackendKind::Local => "file",
            StorageBackendKind::S3 => "s3",
            StorageBackendKind::Gcs => "gs",
            StorageBackendKind::Azure => "az",
        };
        if uri.scheme() != scheme {
            return Err(refused("the URI does not use the storage backend's scheme"));
        }
        let path = tenant_path::strip_bucket(scheme, &uri)
            .map_err(|_| refused("the URI names no storage object"))?;
        let path = tenant_path::validate(path, self.tenant)
            .map_err(|_| refused("the object is outside the tenant's storage"))?;
        let too_large = "the object exceeds the media size limit";
        let size = self
            .storage
            .object_len(&path)
            .await
            .map_err(|error| self.storage_failed(binding, &error))?;
        if size > MEDIA_LIMIT_BYTES {
            return Err(refused(too_large));
        }
        let data = self
            .storage
            .get_object_bounded(&path, MEDIA_LIMIT_BYTES)
            .await
            .map_err(|error| self.storage_failed(binding, &error))?;
        if u64::try_from(data.len()).map_or(true, |len| len > MEDIA_LIMIT_BYTES) {
            return Err(refused(too_large));
        }
        Ok(skald_spec::MediaRef {
            kind: media.kind,
            source: skald_spec::MediaSource::Base64 {
                mime_type,
                data: base64::engine::general_purpose::STANDARD.encode(data),
            },
        })
    }
}

/// Whether `mime` is a provider-supported type for `kind`.
fn supported(kind: skald_spec::MediaKind, mime: &str) -> bool {
    match kind {
        skald_spec::MediaKind::Image => matches!(
            mime,
            "image/png" | "image/jpeg" | "image/gif" | "image/webp"
        ),
        skald_spec::MediaKind::Document => matches!(mime, "application/pdf" | "text/plain"),
    }
}

/// Record media resolution against local tenant storage.
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use opendal::raw::{
        Access, Layer, LayeredAccess, OpList, OpRead, OpStat, OpWrite, RpDelete, RpList, RpRead,
        RpStat, RpWrite,
    };
    use vala_eval::JudgeError;
    use vala_eval::orchestrator::MediaResolver;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::vala::eval::media::MediaRef as EvalMediaRef;
    use wyrd_storage::{
        BackendConfig, BackendSigner, LocalSigner, StorageHandle, StorageSettings, tenant_path,
    };

    use wyrd_spec::ids::VerificationRunId;

    use super::{EngineOutcome, MEDIA_LIMIT_BYTES, RECORD_UNAVAILABLE, ReadError, TenantMedia};

    /// Private object name a media URI carries; it must reach no reason or log.
    const LOCATOR_SENTINEL: &str = "LOCATOR-SENTINEL-8a2d";
    /// Dependency text a failed read carries; it must reach only the log.
    const SQL_SENTINEL: &str = "SQL-SENTINEL-c93e";

    /// In-memory log sink behind a thread-scoped `fmt` subscriber.
    #[derive(Clone, Default)]
    struct Logs(Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for Logs {
        /// Append formatted log bytes to the shared buffer.
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .expect("the log buffer is not poisoned")
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        /// Nothing is buffered outside the shared vector.
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Logs {
        /// Capture every event of the current thread until the guard drops.
        fn capture(&self) -> tracing::subscriber::DefaultGuard {
            let sink = self.clone();
            tracing::subscriber::set_default(
                tracing_subscriber::fmt()
                    .with_ansi(false)
                    .with_max_level(tracing::Level::TRACE)
                    .with_writer(move || sink.clone())
                    .finish(),
            )
        }

        /// Everything captured so far.
        fn text(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().expect("the log buffer is not poisoned"))
                .into_owned()
        }
    }

    /// One `shot` descriptor naming `uri` with `media_type`.
    fn descriptor(uri: &str, media_type: &str) -> EvalMediaRef {
        serde_json::from_value(serde_json::json!({
            "id": "shot", "kind": "image", "uri": uri, "media_type": media_type
        }))
        .expect("the descriptor fixture is valid")
    }

    /// An authorized image resolves to its bytes as native base64; an
    /// unsupported MIME type, foreign scheme, cross-tenant prefix, absent
    /// object, and oversized object are each terminal input errors.
    #[tokio::test]
    async fn media_resolves_only_authorized_bounded_supported_objects() {
        let root = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(StorageHandle::new(BackendSigner::Local(
            LocalSigner::new(root.path().to_path_buf()).expect("local signer"),
        )));
        let tenant = DataTenantId::new_v7();
        let card = uuid::Uuid::now_v7();
        let object = |name: &str| format!("{tenant}/cards/{card}/{name}");
        let path = |name: &str| tenant_path::validate(&object(name), tenant).expect("valid path");
        storage
            .put_object(&path("shot.png"), b"png-bytes".to_vec())
            .await
            .expect("the image writes");
        storage
            .put_object(&path("huge.png"), Vec::new())
            .await
            .expect("the oversized object writes");
        std::fs::File::options()
            .write(true)
            .open(root.path().join(object("huge.png")))
            .and_then(|file| file.set_len(MEDIA_LIMIT_BYTES + 1))
            .expect("the object grows past the limit");
        let media = TenantMedia { storage, tenant };

        let resolved = media
            .resolve(&descriptor(
                &format!("file:///{}", object("shot.png")),
                "image/png",
            ))
            .await
            .expect("the authorized image resolves");
        assert_eq!(
            serde_json::to_value(&resolved.source).expect("source serializes")["data"],
            "cG5nLWJ5dGVz",
            "the provider receives the bytes, not the URI"
        );
        for (uri, mime) in [
            (format!("file:///{}", object("shot.png")), "image/tiff"),
            (format!("s3://bucket/{}", object("shot.png")), "image/png"),
            (
                format!("file:///{}/cards/{card}/shot.png", uuid::Uuid::now_v7()),
                "image/png",
            ),
            (format!("file:///{}", object("absent.png")), "image/png"),
            (format!("file:///{}", object("huge.png")), "image/png"),
        ] {
            assert!(
                matches!(
                    media.resolve(&descriptor(&uri, mime)).await,
                    Err(JudgeError::Terminal { .. })
                ),
                "{uri} as {mime} must be refused"
            );
        }
    }

    /// A media URI's private object key reaches neither the judge error nor
    /// the log: an absent object, a foreign tenant's object, and a storage
    /// read failure (the key names a directory) each yield only the binding
    /// ID and a fixed category, and a read failure stays retryable.
    #[tokio::test]
    async fn media_failures_name_only_the_binding_and_a_category() {
        let logs = Logs::default();
        let _capture = logs.capture();
        let root = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(StorageHandle::new(BackendSigner::Local(
            LocalSigner::new(root.path().to_path_buf()).expect("local signer"),
        )));
        let tenant = DataTenantId::new_v7();
        let card = uuid::Uuid::now_v7();
        let directory = format!("{tenant}/cards/{card}/{LOCATOR_SENTINEL}-dir.png");
        std::fs::create_dir_all(root.path().join(&directory)).expect("the directory exists");
        let media = TenantMedia { storage, tenant };

        let mut reasons = Vec::new();
        for (uri, retryable) in [
            (
                format!("file:///{tenant}/cards/{card}/{LOCATOR_SENTINEL}.png"),
                false,
            ),
            (
                format!(
                    "file:///{}/cards/{card}/{LOCATOR_SENTINEL}.png",
                    uuid::Uuid::now_v7()
                ),
                false,
            ),
            (format!("file:///{directory}"), true),
        ] {
            let reason = match media.resolve(&descriptor(&uri, "image/png")).await {
                Err(JudgeError::Terminal { reason }) if !retryable => reason,
                Err(JudgeError::Retryable { reason }) if retryable => reason,
                other => panic!("{uri} resolved to {:?}", other.map(|_| "content")),
            };
            reasons.push(reason);
        }

        let logs = logs.text();
        assert!(
            reasons
                .iter()
                .all(|reason| reason.starts_with("media `shot`: ")
                    && !reason.contains(LOCATOR_SENTINEL)),
            "reasons name the binding and a category only: {reasons:?}"
        );
        assert!(
            logs.contains("binding=\"shot\"") && !logs.contains(LOCATOR_SENTINEL),
            "the diagnostic log carries no private locator: {logs}"
        );
    }

    /// A failed input read publishes only its stable code and fixed step
    /// text; the dependency cause is kept to the diagnostic log, correlated
    /// by run ID and code.
    #[test]
    fn read_failures_keep_the_cause_in_the_log_only() {
        let logs = Logs::default();
        let _capture = logs.capture();
        let run_id = VerificationRunId::new_v7();

        let outcome = ReadError::Failed(format!("error returned from database: {SQL_SENTINEL}"))
            .outcome(run_id, RECORD_UNAVAILABLE, "the Eval record cannot be read");

        let EngineOutcome::Retry(error) = outcome else {
            panic!("a failed read retries: {outcome:?}");
        };
        assert_eq!(error.code, RECORD_UNAVAILABLE);
        assert_eq!(error.message, "the Eval record cannot be read");
        let logs = logs.text();
        assert!(
            logs.contains(&run_id.to_string())
                && logs.contains(RECORD_UNAVAILABLE)
                && logs.contains(SQL_SENTINEL),
            "the cause is logged with its run and code: {logs}"
        );
    }

    /// Test layer whose `stat` reports every object as one byte long while
    /// reads return the real body: the metadata a replaced or stale object
    /// presents between a size check and the read that follows it.
    #[derive(Debug, Clone, Copy)]
    struct UndersizedStat;

    impl<A: Access> Layer<A> for UndersizedStat {
        type LayeredAccess = UndersizedStatAccess<A>;

        /// Wrap `inner` so only its metadata lies.
        fn layer(&self, inner: A) -> Self::LayeredAccess {
            UndersizedStatAccess(inner)
        }
    }

    /// Accessor of [`UndersizedStat`]: forwards everything except `stat`.
    #[derive(Debug)]
    struct UndersizedStatAccess<A>(A);

    impl<A: Access> LayeredAccess for UndersizedStatAccess<A> {
        type Inner = A;
        type Reader = A::Reader;
        type Writer = A::Writer;
        type Lister = A::Lister;
        type Deleter = A::Deleter;
        type Copier = A::Copier;

        /// The wrapped accessor.
        fn inner(&self) -> &A {
            &self.0
        }

        /// Forward the read unchanged, so the real body is returned.
        async fn read(&self, path: &str, args: OpRead) -> opendal::Result<(RpRead, A::Reader)> {
            self.0.read(path, args).await
        }

        /// Forward the write unchanged.
        async fn write(&self, path: &str, args: OpWrite) -> opendal::Result<(RpWrite, A::Writer)> {
            self.0.write(path, args).await
        }

        /// Forward the delete unchanged.
        async fn delete(&self) -> opendal::Result<(RpDelete, A::Deleter)> {
            self.0.delete().await
        }

        /// Forward the listing unchanged.
        async fn list(&self, path: &str, args: OpList) -> opendal::Result<(RpList, A::Lister)> {
            self.0.list(path, args).await
        }

        /// Report the real metadata with a one-byte content length.
        async fn stat(&self, path: &str, args: OpStat) -> opendal::Result<RpStat> {
            let metadata = self.0.stat(path, args).await?.into_metadata();
            Ok(RpStat::new(metadata.with_content_length(1)))
        }
    }

    /// Metadata is not the authority for media size: an object whose `stat`
    /// reports one byte while its body exceeds [`MEDIA_LIMIT_BYTES`] is
    /// refused as a terminal input error from a body read that stops at the
    /// limit plus one byte, so no provider call can receive it.
    #[tokio::test]
    async fn media_refuses_a_body_past_the_limit_its_metadata_hides() {
        let root = tempfile::tempdir().expect("temp dir");
        let backend = BackendConfig::Local {
            root: root.path().to_path_buf(),
        };
        let operator = wyrd_storage::factory::build_operator(&backend)
            .expect("the local operator builds")
            .layer(UndersizedStat);
        let storage = StorageHandle::from_settings_with_operator(
            StorageSettings {
                backend,
                require_encryption: false,
                presign_ttl: std::time::Duration::from_secs(600),
                part_size_bytes: 16 * 1024 * 1024,
                multipart_threshold_bytes: 100 * 1024 * 1024,
            },
            operator,
        )
        .await
        .expect("the storage handle builds");
        let tenant = DataTenantId::new_v7();
        let object = format!("{tenant}/cards/{}/huge.png", uuid::Uuid::now_v7());
        let path = tenant_path::validate(&object, tenant).expect("valid path");
        storage
            .put_object(&path, Vec::new())
            .await
            .expect("the object writes");
        std::fs::File::options()
            .write(true)
            .open(root.path().join(&object))
            .and_then(|file| file.set_len(MEDIA_LIMIT_BYTES + 1024))
            .expect("the body grows past the limit");
        assert_eq!(
            storage.object_len(&path).await.expect("metadata reads"),
            1,
            "metadata reports an allowed size"
        );
        let limit = usize::try_from(MEDIA_LIMIT_BYTES).expect("the limit fits usize");
        assert_eq!(
            storage
                .get_object_bounded(&path, MEDIA_LIMIT_BYTES)
                .await
                .expect("the bounded body reads")
                .len(),
            limit + 1,
            "the bounded read retains at most the limit plus one byte"
        );
        let media = TenantMedia { storage, tenant };

        let refused = media
            .resolve(&descriptor(&format!("file:///{object}"), "image/png"))
            .await;

        assert!(
            matches!(refused, Err(JudgeError::Terminal { .. })),
            "a body past the limit must be refused before any provider call, got {:?}",
            refused.map(|_| "resolved content")
        );
    }
}
