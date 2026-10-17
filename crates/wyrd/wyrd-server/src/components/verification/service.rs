//! Verification control-plane operations shared by HTTP and MCP.
//!
//! [`VerificationControl`] is the one owner of evidence planning, direct
//! execution, background submission, execution status, and binding status.
//! Both transports hand it an authenticated [`Caller`] and typed input, so
//! authorization, audit, tenancy, idempotency, and error mapping have exactly
//! one implementation.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use arrow::record_batch::RecordBatch;
use chrono::Utc;
use tokio_util::sync::CancellationToken;
use tracing::Instrument as _;
use vala_verify::blocks::{DriftChecks, EngineError};
use wyrd_runtime::outbox::{OutboxFull, Reservation};
use wyrd_runtime::permission::{Action, Resource};
use wyrd_runtime::{AcceptedAuthority, CardAttribution, Permission};
use wyrd_spec::auth::{PrincipalId, Sha256Hex};
use wyrd_spec::card::verifier::{CheckEvidence, VerifierSpec};
use wyrd_spec::envelope::Spec;
use wyrd_spec::error::WyrdError;
use wyrd_spec::error::storage::WyrdStorageError;
use wyrd_spec::ids::{BindingId, CardUid, ExecutionId, IdempotencyKey, VerificationResultId};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::api::{AuditOutcome, BifrostQueryRequest};
use wyrd_spec::vala::ids::{RunId, SpanId, TraceId};
use wyrd_spec::verification::VerificationBindingStatus;
use wyrd_spec::verification::execution::{
    EnqueueVerificationResponse, EvidencePlanRequest, EvidencePlanResponse, EvidenceUpload,
    VerificationExecutionState, VerificationOutcome, VerificationRequest, VerificationResult,
};
use wyrd_sql::queries::auth::system_principal_id;
use wyrd_sql::queries::cards::get_card_by_uid;
use wyrd_sql::queries::verifier_executions::{Cause, Submission, VerifierExecutionQueue};
use wyrd_sql::{CardStatus, ParsedCardRow, TenantConn};
use wyrd_storage::StorageError;
use wyrd_storage::tenant_path;

use crate::audit;
use crate::components::auth::Caller;
use crate::components::gateway::GatewayCallSubject;
use crate::http::error::permission_deny_reason_to_wyrd;
use crate::http::middleware::edge_timeout::EdgeTimer;
use crate::query::scheduled::ScheduledQueryCaller;
use crate::scribe_outbox::{ScribeSink, ScribeWrite};
use crate::state::{AppState, registry_db_error};
use crate::verification::hosted::blocks::{Execution, HostedBlocks};
use crate::verification::results::{ResultPayloadBuilder, ResultPayloadError, ResultRun};
use crate::verification::telemetry::{ExecutionMode, ExecutionTelemetry, Phase};

/// Audit operation of a binding status read.
const GET_BINDING: &str = "verification.binding.read";

/// Audit operation of an execution status read.
const GET_EXECUTION: &str = "verification.execution.read";

/// Audit operation of an evidence upload plan.
const EVIDENCE: &str = "verification.evidence";

/// Audit operation of a local-backend evidence body upload.
const EVIDENCE_UPLOAD: &str = "verification.evidence.upload";

/// Audit operation of a direct execution.
const EXECUTE: &str = "verification.execute";

/// Audit operation of a background submission.
const ENQUEUE: &str = "verification.enqueue";

/// Lifetime of one evidence upload URL.
const EVIDENCE_UPLOAD_TTL: Duration = Duration::from_mins(10);

/// Stable query error of a tenant result table no result has created yet.
const RESULTS_NOT_CREATED: &str = "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND";

/// The exact Verifier and subject a direct execution resolved.
struct DirectTarget {
    /// Exact Verifier reference.
    verifier: CardRef,
    /// The Verifier's building-block spec.
    spec: Arc<VerifierSpec>,
    /// Exact subject reference.
    subject: CardRef,
    /// The tenant's active SYSTEM principal, which executes the Verifier.
    system_principal: Option<PrincipalId>,
}

/// The exact reference of a stored Card row.
fn exact_ref(row: &ParsedCardRow) -> CardRef {
    CardRef {
        kind: row.kind.clone(),
        name: row.name.clone(),
        version: row.version.clone(),
        space: Some(row.space.clone()),
        uid: Some(row.card_uid.clone()),
    }
}

/// Whether a stored Card may be verified or verify now.
fn available(row: &ParsedCardRow) -> bool {
    matches!(row.status, CardStatus::Active | CardStatus::Deprecated)
}

/// Map an inputs or result encoding failure to its public error.
///
/// A Variant limit is the caller's oversized input; every other failure is
/// a server defect.
fn payload_error(error: &ResultPayloadError) -> WyrdError {
    match error {
        ResultPayloadError::Variant(_) => WyrdError::VerificationInputTooLarge {
            message: format!("the verification inputs cannot be recorded: {error}"),
            details: serde_json::json!({}),
        },
        _ => WyrdError::internal(format!(
            "the verification record cannot be encoded: {error}"
        )),
    }
}

/// Seconds a caller refused staging capacity waits before retrying.
const STAGING_RETRY_AFTER_SECONDS: u64 = 1;

/// Map a refused outbox reservation to the caller's retryable pressure.
///
/// The refusal admitted nothing, so the identical call may be retried whole.
fn staging_full(full: OutboxFull) -> WyrdError {
    tracing::warn!(outbox = full.outbox, admitted = full.admitted, limit = full.limit, "verification staging refused");
    metrics::counter!("verification_staging_refused_total", "outbox" => full.outbox).increment(1);
    WyrdError::VerificationStagingFull {
        message: "this server has no verification staging capacity now".to_owned(),
        details: serde_json::json!({ "retry_after_seconds": STAGING_RETRY_AFTER_SECONDS }),
    }
}

/// Owner of the Verification control plane for one request.
///
/// Borrows the process [`AppState`] for its tenant connections, permission
/// checker, hosted blocks, storage, and outboxes. It holds no connection of
/// its own: each read opens one tenant transaction under forced RLS, so
/// another tenant's bindings and executions are indistinguishable from
/// absent ones. Direct execution and submission write nothing to Postgres.
pub(crate) struct VerificationControl<'a> {
    /// Server state providing tenant connections, authorization, and audit.
    state: &'a AppState,
    /// Durable execution queue read for status.
    queue: VerifierExecutionQueue,
}

impl<'a> VerificationControl<'a> {
    /// Build the control plane over this process's state.
    pub(crate) fn new(state: &'a AppState) -> Self {
        Self {
            state,
            queue: VerifierExecutionQueue::default(),
        }
    }

    /// Read one binding's identities, activity gate, readiness, and cursor.
    ///
    /// Authorizes and audits `cards:read` standalone, then reads under the
    /// caller's tenant.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] without `cards:read`,
    /// [`WyrdError::VerificationBindingNotFound`] when the tenant has no such
    /// binding, and a registry unavailability error when the read fails.
    pub(crate) async fn get_binding(
        &self,
        caller: &Caller,
        binding_id: BindingId,
    ) -> Result<VerificationBindingStatus, WyrdError> {
        let resource = format!("verification.binding:{binding_id}");
        audit::authorize(
            self.state,
            caller,
            &Permission::card_read(),
            GET_BINDING,
            &resource,
        )?;
        let mut conn = self
            .state
            .registry_tenant_conn(caller.data_tenant_id)
            .await?;
        let status = self
            .queue
            .binding_status(&mut conn, binding_id)
            .await
            .map_err(registry_db_error)?;
        conn.commit().await.map_err(registry_db_error)?;
        status.ok_or_else(|| WyrdError::VerificationBindingNotFound {
            message: "no verification binding with this ID in the caller's tenant".to_owned(),
            details: serde_json::json!({ "binding_id": binding_id }),
        })
    }

    /// Read one execution's status, dispatches, and result once visible.
    ///
    /// Authorizes and audits `cards:read` standalone, reads the status and
    /// both exact Cards in one tenant transaction, then — for a completed
    /// execution — reads its result from `vala.verification.results` as the
    /// caller through the ordinary Bifrost query entry. A result Scribe has
    /// not yet published leaves `result` absent, so the caller polls again.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] without `cards:read`,
    /// [`WyrdError::VerificationExecutionNotFound`] when the tenant has no
    /// such execution, a registry unavailability error when a read fails, and
    /// the query error when the result read is refused or fails.
    pub(crate) async fn get_execution(
        &self,
        caller: &Caller,
        execution_id: ExecutionId,
    ) -> Result<VerificationExecutionState, WyrdError> {
        let resource = format!("verification.execution:{execution_id}");
        audit::authorize(
            self.state,
            caller,
            &Permission::card_read(),
            GET_EXECUTION,
            &resource,
        )?;
        let mut conn = self
            .state
            .registry_tenant_conn(caller.data_tenant_id)
            .await?;
        let status = self
            .queue
            .execution_status(&mut conn, execution_id)
            .await
            .map_err(registry_db_error)?
            .ok_or_else(|| WyrdError::VerificationExecutionNotFound {
                message: "no verification execution with this ID in the caller's tenant".to_owned(),
                details: serde_json::json!({ "execution_id": execution_id }),
            })?;
        let mut state = status.state;
        let Some(result_id) = status.result_id else {
            return Ok(state);
        };
        let verifier = Self::card(&mut conn, &status.verifier_card_uid).await?;
        let subject = Self::card(&mut conn, &status.card_uid).await?;
        drop(conn);
        let (Some(verifier), Some(subject)) = (verifier, subject) else {
            return Ok(state);
        };
        state.result = self
            .read_result(caller, result_id)
            .await?
            .map(|(run_id, outcome)| VerificationResult {
                execution_id,
                run_id,
                verifier: exact_ref(&verifier),
                subject: exact_ref(&subject),
                outcome,
            });
        Ok(state)
    }

    /// Read one published result's application Run and outcome as `caller`.
    ///
    /// `Ok(None)` when no row is visible yet, including a tenant whose result
    /// table no result has created.
    ///
    /// # Errors
    /// Returns the query refusal or failure, and an internal error when the
    /// stored outcome does not decode.
    async fn read_result(
        &self,
        caller: &Caller,
        result_id: VerificationResultId,
    ) -> Result<Option<(Option<RunId>, VerificationOutcome)>, WyrdError> {
        let query = ScheduledQueryCaller::authenticated(
            self.state.clone(),
            caller.clone(),
            CancellationToken::new(),
        )?;
        let mut batches: Vec<RecordBatch> = Vec::new();
        let request = BifrostQueryRequest {
            params: Vec::new(),
            sql: format!(
                "SELECT outcome, run_id FROM vala.verification.results \
                 WHERE result_id = '{result_id}' LIMIT 1"
            ),
            deadline_ms: None,
        };
        let read = query.run_with(request, |batch| {
            batches.push(batch);
            Ok(())
        });
        match read.await {
            Err(error) if error.code() == RESULTS_NOT_CREATED => return Ok(None),
            Err(error) => return Err(error),
            Ok(_) => {}
        }
        let Some(batch) = batches.iter().find(|batch| batch.num_rows() > 0) else {
            return Ok(None);
        };
        let decode =
            |error: String| WyrdError::internal(format!("stored result is invalid: {error}"));
        let column = |name: &str| {
            batch
                .column_by_name(name)
                .ok_or_else(|| decode(format!("no {name} column")))
        };
        let outcome = wyrd_queue::variant::variant_cell_to_json(column("outcome")?.as_ref(), 0)
            .map_err(|error| decode(format!("{error:?}")))?;
        let outcome = serde_json::from_value(outcome).map_err(|error| decode(error.to_string()))?;
        let run = column("run_id")?;
        let run_id = if run.is_null(0) {
            None
        } else {
            let run = arrow::util::display::array_value_to_string(run, 0)
                .map_err(|error| decode(error.to_string()))?;
            Some(RunId::from_string(run))
        };
        Ok(Some((run_id, outcome)))
    }

    /// Plan uploads of the caller's evidence files that storage lacks.
    ///
    /// Admits and audits any `verifier:run` grant, refuses a file
    /// or request larger than the Deployment limits, and returns one
    /// single-PUT plan for each digest whose object
    /// `<tenant>/evidence/sha256/<digest>` is absent or of another length.
    /// Writes nothing: the bytes go straight to tenant storage, and readers
    /// check digest and length again.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] without any `verifier:run` grant,
    /// [`WyrdError::VerificationInputTooLarge`] past a limit,
    /// [`WyrdError::ServiceUnavailable`] when this process hosts no Verifier
    /// blocks, and the storage error when a lookup or signature fails.
    pub(crate) async fn evidence(
        &self,
        caller: &Caller,
        request: &EvidencePlanRequest,
    ) -> Result<EvidencePlanResponse, WyrdError> {
        let limits = *self.blocks()?.limits();
        self.authorize_evidence(caller, EVIDENCE)?;
        let total: u64 = request.files.iter().map(|file| file.size_bytes).sum();
        if let Some(file) = request
            .files
            .iter()
            .find(|file| file.size_bytes > limits.file_max_bytes)
        {
            return Err(WyrdError::VerificationInputTooLarge {
                message: format!(
                    "evidence files may be at most {} bytes",
                    limits.file_max_bytes
                ),
                details: serde_json::json!({ "sha256": file.sha256, "max_bytes": limits.file_max_bytes }),
            });
        }
        if total > limits.files_max_bytes {
            return Err(WyrdError::VerificationInputTooLarge {
                message: format!(
                    "one request's evidence may be at most {} bytes",
                    limits.files_max_bytes
                ),
                details: serde_json::json!({ "max_bytes": limits.files_max_bytes }),
            });
        }
        let storage_error = |error: StorageError| WyrdError::from(WyrdStorageError::from(error));
        let mut uploads = Vec::new();
        for file in &request.files {
            let path = tenant_path::evidence(caller.data_tenant_id, &file.sha256);
            match self.state.storage.object_len(&path).await {
                Ok(len) if len == file.size_bytes => continue,
                Ok(_) | Err(StorageError::ObjectNotFound { .. }) => {}
                Err(error) => return Err(storage_error(error)),
            }
            let plan = wyrd_storage::service::evidence_upload_plan(
                &self.state.storage,
                &path,
                &file.sha256,
                file.size_bytes,
                EVIDENCE_UPLOAD_TTL,
            )
            .await
            .map_err(storage_error)?;
            uploads.push(EvidenceUpload {
                sha256: file.sha256,
                plan,
            });
        }
        Ok(EvidencePlanResponse { uploads })
    }

    /// Store one evidence body on the local backend, where an evidence plan
    /// is the authenticated route rather than a presigned URL.
    ///
    /// Admits and audits any `verifier:run` grant like
    /// [`Self::evidence`], reads at most the Deployment `file_max_bytes`, and
    /// writes the body to `<tenant>/evidence/sha256/<digest>` only when it
    /// hashes to `sha256`. Writes nothing to Postgres.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] without any `verifier:run` grant,
    /// [`WyrdError::VerificationInputTooLarge`] past `file_max_bytes`,
    /// [`WyrdError::ServiceUnavailable`] when this process hosts no Verifier
    /// blocks, the storage `Sha256Mismatch` error for a body of another
    /// digest, and the storage error when the write fails.
    pub(crate) async fn upload_local_evidence(
        &self,
        caller: &Caller,
        sha256: &Sha256Hex,
        body: axum::body::Body,
    ) -> Result<(), WyrdError> {
        let max_bytes = self.blocks()?.limits().file_max_bytes;
        self.authorize_evidence(caller, EVIDENCE_UPLOAD)?;
        let too_large = || WyrdError::VerificationInputTooLarge {
            message: format!("evidence files may be at most {max_bytes} bytes"),
            details: serde_json::json!({ "sha256": sha256, "max_bytes": max_bytes }),
        };
        let limit = usize::try_from(max_bytes).map_err(|_| too_large())?;
        let body = axum::body::to_bytes(body, limit)
            .await
            .map_err(|_| too_large())?;
        wyrd_storage::service::upload_local_evidence(
            &self.state.storage,
            caller.data_tenant_id,
            sha256,
            &body,
        )
        .await
    }

    /// Execute one exact Verifier over supplied evidence and return its
    /// result.
    ///
    /// Evaluates `verifier:run` on the requested Verifier UID, then resolves
    /// both Cards in the caller's tenant; an unknown, inactive, or
    /// foreign-tenant Card is not found, and a Window (drift) Verifier is
    /// refused because only its schedule judges a window. The engine runs
    /// the hosted blocks as the caller, executed by the tenant SYSTEM
    /// principal, under the Deployment `verify_timeout`. Permission blocks;
    /// audit does not: the single allow or deny decision is staged on
    /// [`AppState::scribe_outbox`]. The pinned inputs (never queueing work)
    /// and the result are staged there too; nothing is written to Postgres
    /// and no Operator fires. Dropping the future cancels the execution. An
    /// HTTP caller passes its [`EdgeTimer`], handed off once the target is
    /// resolved so the deadline, not the generic edge timeout, governs the
    /// engine; MCP passes `None`.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] without `verifier:run` on
    /// the Verifier, [`WyrdError::VerificationTargetNotFound`] for an
    /// unknown, inactive, or non-Verifier target,
    /// [`WyrdError::VerificationInputUnsupported`] for a Window Verifier,
    /// [`WyrdError::VerificationInputInvalid`] for evidence that does not
    /// conform to the Verifier's input slots,
    /// [`WyrdError::VerificationInputTooLarge`] for evidence past a slot or
    /// combined input-file limit,
    /// [`WyrdError::VerificationExecutionTimedOut`] past the deadline,
    /// [`WyrdError::ServiceUnavailable`] when this process hosts no Verifier
    /// blocks, a block is refused process capacity (a Python worker or a
    /// workspace), or evidence metadata cannot be read now, a registry
    /// unavailability error when a read fails, and the Verifier's planning
    /// refusal.
    #[tracing::instrument(
        name = "verification.execute",
        skip_all,
        fields(execution_id, mode = "direct", outcome)
    )]
    pub(crate) async fn execute(
        &self,
        caller: &Caller,
        request: &VerificationRequest,
        edge_timer: Option<&EdgeTimer>,
    ) -> Result<VerificationResult, WyrdError> {
        let blocks = self.blocks()?;
        let execution_id = ExecutionId::new_v7();
        let span = tracing::Span::current();
        span.record("execution_id", tracing::field::display(execution_id));
        let resource = format!(
            "verifier:{}/subject:{}/execution:{execution_id}",
            request.verifier_card_uid, request.card_uid
        );
        self.authorize_run(caller, &request.verifier_card_uid, None, EXECUTE, &resource)?;
        // The inputs and result rows, reserved before any work is done.
        let staged = self.state.scribe_outbox.reserve(2).map_err(staging_full)?;
        let telemetry = ExecutionTelemetry::start(ExecutionMode::Direct);
        let target = telemetry
            .phase(
                Phase::Load,
                self.resolve(caller, request)
                    .instrument(tracing::info_span!("verification.load")),
            )
            .await?;
        if let Some(edge_timer) = edge_timer {
            edge_timer.hand_off();
        }
        let system = target.system_principal.ok_or_else(|| {
            WyrdError::internal("the tenant has no active SYSTEM principal to execute the Verifier")
        })?;
        let executor = caller.clone().executed_by_system(system);
        let started_at = Utc::now();
        let timeout = blocks.limits().verify_timeout;
        let engine = blocks.engine(
            self.state,
            Execution {
                caller: &executor,
                evidence: &request.inputs,
                routes: &request.execution_route_overrides,
                subject: GatewayCallSubject {
                    run_id: request.run_id.clone(),
                    card_uid: Some(request.card_uid.clone()),
                },
                accepted_at: started_at,
                drift: DriftChecks::new(None, BTreeMap::new(), BTreeMap::new()),
            },
        )?;
        let ran = telemetry
            .phase(
                Phase::Engine,
                engine.run(
                    Arc::clone(&target.spec),
                    request.inputs.clone(),
                    tokio::time::Instant::now() + timeout,
                ),
            )
            .instrument(tracing::info_span!("verification.engine"))
            .await;
        let (label, result) = match ran {
            Ok(outcome) => ("completed", Ok(outcome)),
            Err(EngineError::DeadlineExceeded) => (
                "timed_out",
                Err(WyrdError::VerificationExecutionTimedOut {
                    message: "the Verifier exceeded the direct execution deadline".to_owned(),
                    details: serde_json::json!({ "deadline_seconds": timeout.as_secs() }),
                }),
            ),
            Err(EngineError::Plan(error)) => ("errored", Err(error)),
            Err(EngineError::Busy(reason)) => (
                "unavailable",
                Err(WyrdError::ServiceUnavailable {
                    message: "the verification runtime has no capacity for this Verifier now"
                        .to_owned(),
                    details: serde_json::json!({ "reason": reason }),
                }),
            ),
            Err(EngineError::InputInvalid(reason)) => (
                "refused",
                Err(WyrdError::VerificationInputInvalid {
                    message: reason.clone(),
                    details: serde_json::json!({ "reason": reason }),
                }),
            ),
            Err(EngineError::InputTooLarge(reason)) => (
                "refused",
                Err(WyrdError::VerificationInputTooLarge {
                    message: reason.clone(),
                    details: serde_json::json!({ "reason": reason }),
                }),
            ),
            Err(EngineError::EvidenceUnavailable(reason)) => (
                "unavailable",
                Err(WyrdError::ServiceUnavailable {
                    message: "the verification evidence cannot be admitted now".to_owned(),
                    details: serde_json::json!({ "reason": reason }),
                }),
            ),
            Err(error) => ("errored", Err(WyrdError::internal(error.to_string()))),
        };
        span.record("outcome", label);
        telemetry.finish(label, label != "completed");
        let outcome = result?;
        Self::stage_direct(&staged, caller, execution_id, request, &target, &outcome, started_at);
        Ok(VerificationResult {
            execution_id,
            run_id: request.run_id.clone(),
            verifier: target.verifier,
            subject: target.subject,
            outcome,
        })
    }

    /// Stage one background execution and return its identity.
    ///
    /// Evaluates `verifier:run` on the requested Verifier UID, assigns the
    /// execution ID (derived from `key` and the canonical request when keyed,
    /// so identical retries share it), and stages the pinned inputs marked
    /// `queue_execution` on the Scribe outbox with the [`Submission`]. Once
    /// Scribe acknowledges the inputs, the submission is queued through the
    /// batched writer, which decides replay, conflict, and refusal; until
    /// then status reads not found. Reads and writes no Postgres. The
    /// execution runs as the caller, never as a binding owner. The request's
    /// span and the caller's own sampled flag become the execution's
    /// [`Cause`], which its attempts link to.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] without `verifier:run` on
    /// the Verifier or when a Card-bound caller's signed scope does not carry
    /// the subject's UID, [`WyrdError::VerificationInputTooLarge`] for inputs
    /// past a Variant limit, and [`WyrdError::ServiceUnavailable`] when this
    /// process hosts no Verifier queue.
    pub(crate) fn enqueue(
        &self,
        caller: &Caller,
        request: VerificationRequest,
        key: Option<&IdempotencyKey>,
    ) -> Result<EnqueueVerificationResponse, WyrdError> {
        let submissions = self
            .state
            .bifrost
            .submissions()
            .filter(|_| self.state.scribe_outbox.sink().reaches_scribe())
            .ok_or_else(Self::unavailable)?;
        let resource = format!(
            "verifier:{}/subject:{}",
            request.verifier_card_uid, request.card_uid
        );
        self.authorize_run(
            caller,
            &request.verifier_card_uid,
            Some(&request.card_uid),
            ENQUEUE,
            &resource,
        )?;
        let cause = wyrd_telemetry::current_context().and_then(|context| {
            Some(Cause {
                trace_id: TraceId::from_hex(&context.trace_id).ok()?,
                span_id: SpanId::from_hex(&context.span_id).ok()?,
                sampled: wyrd_telemetry::caller_sampled(),
            })
        });
        let submission = Submission::new(
            caller.data_tenant_id,
            AcceptedAuthority::new(&caller.principal, &caller.delegation_chain),
            request,
            cause,
            key.map(|key| key.as_str().to_owned()),
        )
        .map_err(|error| {
            WyrdError::internal(format!("the request cannot be canonicalized: {error}"))
        })?;
        let now = Utc::now();
        let row = ResultPayloadBuilder::new(
            ResultRun {
                execution_id: submission.execution_id,
                run_id: submission.request.run_id.as_ref(),
                // The inputs row records no Verifier version.
                verifier_version: "",
                card_uid: &submission.request.card_uid,
                owner_card_uid: None,
                binding_id: None,
                trigger: None,
                window: None,
            },
            &submission.request.verifier_card_uid,
            now,
            now,
        )
        .inputs(&submission.request.inputs, Some(&submission))
        .map_err(|error| payload_error(&error))?;
        let queued = submissions.reserve(1).map_err(staging_full)?;
        let staged = self.state.scribe_outbox.reserve(1).map_err(staging_full)?;
        let execution_id = submission.execution_id;
        staged.stage(
            caller.data_tenant_id,
            ScribeWrite::ExecutionInputs {
                row,
                attribution: caller.attribution(),
                queued: Some((queued, Box::new(submission))),
            },
        );
        Ok(EnqueueVerificationResponse { execution_id })
    }

    /// Stage a completed direct execution's inputs and result on the Scribe
    /// outbox.
    ///
    /// Both rows are staged on the slots `staged` reserved before the
    /// execution ran. The payload mints its own result identity and
    /// correlates with the execution through `execution_id`. Its rows name the subject
    /// as `card_uid` and the exact Verifier as `verifier_card_uid`, are
    /// attributed to the caller, carry the caller's application Run, and have
    /// no owner, binding, or Trigger. The inputs row never queues work. The
    /// verdict is already decided, so an unbuildable payload is logged and
    /// not recorded.
    fn stage_direct(
        staged: &Reservation<ScribeSink>,
        caller: &Caller,
        execution_id: ExecutionId,
        request: &VerificationRequest,
        target: &DirectTarget,
        outcome: &VerificationOutcome,
        started_at: chrono::DateTime<Utc>,
    ) {
        let verifier_version = target.verifier.version.to_string();
        let builder = ResultPayloadBuilder::new(
            ResultRun {
                execution_id,
                run_id: request.run_id.as_ref(),
                verifier_version: &verifier_version,
                card_uid: &request.card_uid,
                owner_card_uid: None,
                binding_id: None,
                trigger: None,
                window: None,
            },
            &request.verifier_card_uid,
            started_at,
            Utc::now(),
        );
        let built = builder
            .inputs(&request.inputs, None)
            .and_then(|row| Ok((row, builder.build(outcome)?)));
        match built {
            Ok((row, payload)) => {
                staged.stage(
                    caller.data_tenant_id,
                    ScribeWrite::ExecutionInputs {
                        row,
                        attribution: caller.attribution(),
                        queued: None,
                    },
                );
                staged.stage(
                    caller.data_tenant_id,
                    ScribeWrite::Result {
                        payload,
                        attribution: caller.attribution(),
                    },
                );
            }
            Err(error) => {
                tracing::warn!(%execution_id, %error, "the direct result cannot be encoded; it is not recorded");
            }
        }
    }

    /// Resolve a direct execution's Verifier and subject Cards.
    ///
    /// Reads both Cards and the tenant SYSTEM principal in one read-only
    /// tenant transaction, so a Card of another tenant is invisible. Both
    /// must be active and the target must be a Run-evidence Verifier Card.
    ///
    /// # Errors
    /// Returns a registry error when a read fails,
    /// [`WyrdError::VerificationTargetNotFound`] for an unknown, inactive, or
    /// non-Verifier target, and [`WyrdError::VerificationInputUnsupported`]
    /// for a Window Verifier.
    async fn resolve(
        &self,
        caller: &Caller,
        request: &VerificationRequest,
    ) -> Result<DirectTarget, WyrdError> {
        let mut conn = self
            .state
            .registry_tenant_conn(caller.data_tenant_id)
            .await?;
        let verifier = Self::card(&mut conn, &request.verifier_card_uid).await?;
        let subject = Self::card(&mut conn, &request.card_uid).await?;
        let system_principal = system_principal_id(&mut conn)
            .await
            .map_err(registry_db_error)?
            .map(PrincipalId::new);
        drop(conn);
        let not_found = || WyrdError::VerificationTargetNotFound {
            message: "no active Verifier and subject Card with these UIDs in the caller's tenant"
                .to_owned(),
            details: serde_json::json!({
                "verifier_card_uid": request.verifier_card_uid,
                "card_uid": request.card_uid,
            }),
        };
        let (Some(verifier), Some(subject)) = (verifier, subject) else {
            return Err(not_found());
        };
        if !available(&verifier) || !available(&subject) {
            return Err(not_found());
        }
        let reference = exact_ref(&verifier);
        let Spec::Verifier(spec) = verifier.spec else {
            return Err(not_found());
        };
        if spec
            .all_requirements()
            .any(|requirement| requirement.check.evidence() == CheckEvidence::Window)
        {
            return Err(WyrdError::VerificationInputUnsupported {
                message: "a drift Verifier judges a scheduled window; record samples and submit \
                          them instead of verifying now"
                    .to_owned(),
                details: serde_json::json!({ "verifier_card_uid": request.verifier_card_uid }),
            });
        }
        Ok(DirectTarget {
            verifier: reference,
            spec: Arc::new(spec),
            subject: exact_ref(&subject),
            system_principal,
        })
    }

    /// The process's hosted Verifier blocks.
    ///
    /// # Errors
    /// Returns [`WyrdError::ServiceUnavailable`] when this process composed
    /// none.
    fn blocks(&self) -> Result<&HostedBlocks, WyrdError> {
        self.state
            .verification_blocks
            .as_deref()
            .ok_or_else(Self::unavailable)
    }

    /// The refusal of a process that hosts no verification runtime.
    fn unavailable() -> WyrdError {
        WyrdError::ServiceUnavailable {
            message: "this server process does not host Verifier execution".to_owned(),
            details: serde_json::json!({}),
        }
    }

    /// Evaluate `verifier:run` on `verifier` for `operation` and stage the
    /// single allow or deny decision without waiting for its commit.
    ///
    /// When `subject` is given, a Card-bound caller must also carry that UID
    /// in its signed Card scope; this is the same in-memory rule the gateway
    /// applies to attribution, so no registry read is needed. A caller bound
    /// to no Card is decided by RBAC alone.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] when RBAC refuses the
    /// Verifier or the signed scope does not carry `subject`.
    fn authorize_run(
        &self,
        caller: &Caller,
        verifier: &CardUid,
        subject: Option<&CardUid>,
        operation: &str,
        resource: &str,
    ) -> Result<(), WyrdError> {
        let required = Permission::verifier_run(verifier.clone());
        let decision = self
            .state
            .authz
            .permission_check
            .check(&caller.principal, &required)
            .into_result()
            .map_err(permission_deny_reason_to_wyrd)
            .and_then(|()| match (subject, caller.principal.card_attribution()) {
                (Some(subject), CardAttribution::Scoped(scope))
                    if !scope.uids().contains(subject) =>
                {
                    Err(WyrdError::PermissionDeniedRbac {
                        message: "the caller's Card scope does not cover the verified subject"
                            .to_owned(),
                        details: serde_json::json!({ "resource": resource }),
                    })
                }
                _ => Ok(()),
            });
        let outcome = if decision.is_ok() {
            AuditOutcome::Allowed
        } else {
            AuditOutcome::Denied
        };
        self.state.scribe_outbox.stage(
            caller.data_tenant_id,
            audit::audit_event(caller, operation, resource, &required.to_string(), outcome),
        );
        decision
    }

    /// Admit evidence staging for `operation` and stage the single allow or
    /// deny decision without waiting for its commit.
    ///
    /// Staging selects no Verifier and cannot execute one, so any
    /// `verifier:run` grant admits it, including one scoped to a single
    /// Verifier UID. The exact Verifier is authorized later by
    /// [`Self::authorize_run`] when execute or enqueue names it.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] when the caller holds no
    /// `verifier:run` grant of any scope.
    fn authorize_evidence(&self, caller: &Caller, operation: &str) -> Result<(), WyrdError> {
        let required = Permission::verifier_run_any();
        let allowed = caller
            .principal
            .effective_permissions
            .covers_operation(&Resource::Verifier, &Action::Run);
        let outcome = if allowed {
            AuditOutcome::Allowed
        } else {
            AuditOutcome::Denied
        };
        self.state.scribe_outbox.stage(
            caller.data_tenant_id,
            audit::audit_event(
                caller,
                operation,
                "verification.evidence",
                &required.to_string(),
                outcome,
            ),
        );
        if allowed {
            Ok(())
        } else {
            Err(WyrdError::PermissionDeniedRbac {
                message: format!(
                    "principal {} lacks any verifier:run grant",
                    caller.principal.id
                ),
                details: serde_json::json!({ "required": required, "principal": caller.principal.id.to_string() }),
            })
        }
    }

    /// Read one Card of the caller's tenant, or `None` when it is absent.
    ///
    /// # Errors
    /// Returns a registry error other than not-found.
    async fn card(
        conn: &mut TenantConn<'_>,
        uid: &CardUid,
    ) -> Result<Option<ParsedCardRow>, WyrdError> {
        match get_card_by_uid(conn, uid).await {
            Ok(row) => Ok(Some(row)),
            Err(WyrdError::RegistryCardNotFound { .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }
}
