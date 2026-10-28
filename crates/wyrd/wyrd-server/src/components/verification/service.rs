//! Verification control-plane operations shared by HTTP and MCP.
//!
//! [`VerificationControl`] is the one owner of evidence planning, direct
//! execution, background submission, execution status, and binding status.
//! Both transports hand it an authenticated [`Caller`] and typed input, so
//! authorization, audit, tenancy, idempotency, and error mapping have exactly
//! one implementation.

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::Utc;
use tokio_util::sync::CancellationToken;
use tracing::Instrument as _;
use vala_verify::blocks::{DriftChecks, EngineError};
use wyrd_runtime::outbox::{OutboxFull, Reservation};
use wyrd_runtime::permission::{Action, Resource};
use wyrd_runtime::{AcceptedAuthority, CardAttribution, Permission};
use wyrd_spec::card::verifier::CheckEvidence;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{BindingId, CardUid, ExecutionId, IdempotencyKey};
use wyrd_spec::vala::api::AuditOutcome;
use wyrd_spec::vala::ids::{SpanId, TraceId};
use wyrd_spec::verification::execution::{
    EnqueueVerificationResponse, EvidencePlanRequest, EvidencePlanResponse, EvidenceUpload,
    VerificationExecutionState, VerificationOutcome, VerificationRequest, VerificationResult,
};
use wyrd_spec::verification::{
    VerificationBindingStatus, VerificationExecutionStatus, VerifierReadiness,
};
use wyrd_sql::queries::cards::get_card_by_uid;
use wyrd_sql::queries::verifier_executions::{
    Cause, EnqueueRefusal, Submission, VerifierExecutionQueue,
};
use wyrd_sql::{ParsedCardRow, TenantConn};

use crate::audit;
use crate::components::auth::Caller;
use crate::components::gateway::GatewayCallContext;
use crate::components::storage::inventory::ObjectInventory;
use crate::components::storage::routes::storage_caller;
use crate::http::error::permission_deny_reason_to_wyrd;
use crate::http::middleware::edge_timeout::EdgeTimer;
use crate::http::middleware::metrics::RequestArrival;
use crate::query::scheduled::ScheduledQueryCaller;
use crate::scribe_outbox::{ScribeSink, ScribeWrite};
use crate::state::{AppState, registry_db_error};
use crate::verification::cache::{Db, Target, exact_ref};
use crate::verification::hosted::blocks::{Execution, HostedBlocks};
use crate::verification::inputs::InputResolver;
use crate::verification::results::{
    InputRecord, ResultPayloadBuilder, ResultPayloadError, ResultRun,
};
use crate::verification::telemetry::{ExecutionMode, ExecutionTelemetry, Phase};

/// Audit operation of a binding status read.
const GET_BINDING: &str = "verification.binding.read";

/// Audit operation of an execution status read.
const GET_EXECUTION: &str = "verification.execution.read";

/// Audit operation of an evidence upload plan.
const EVIDENCE: &str = "verification.evidence";

/// Audit operation of a direct execution.
const EXECUTE: &str = "verification.execute";

/// Audit operation of a background submission.
const ENQUEUE: &str = "verification.enqueue";

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
    tracing::warn!(
        outbox = full.outbox,
        admitted = full.admitted,
        limit = full.limit,
        "verification staging refused"
    );
    metrics::counter!("verification_staging_refused_total", "outbox" => full.outbox).increment(1);
    WyrdError::VerificationStagingFull {
        message: "this server has no verification staging capacity now".to_owned(),
        details: serde_json::json!({ "retry_after_seconds": STAGING_RETRY_AFTER_SECONDS }),
    }
}

/// Refuse an empty `deployment_environment` tag.
///
/// The tag is optional correlation context copied onto the accepted
/// execution and its result; it never selects, filters, or changes the
/// execution. An empty value is not a tag, so it is refused at admission
/// rather than stored. Pure: no IO.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` naming `deployment_environment` when the
/// request carries an empty value.
fn environment_tag(request: &VerificationRequest) -> Result<(), WyrdError> {
    if request.deployment_environment.as_deref() == Some("") {
        return Err(WyrdError::Validation {
            message: "deployment_environment must be non-empty or omitted".to_owned(),
            details: serde_json::json!({ "field": "deployment_environment" }),
        });
    }
    Ok(())
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
    /// execution — reads its PostgreSQL-selected result through the one
    /// result reader as the caller, through the ordinary Bifrost query entry.
    /// A group execution's source selection is read from its input record. A
    /// direct execution has no queue row: its ID resolves only through a
    /// published result whose input record has the `direct` origin, and reads
    /// as completed. A result Scribe has not yet published leaves `result`
    /// absent, so the caller polls again.
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
        let query = ScheduledQueryCaller::authenticated(
            self.state.clone(),
            caller.clone(),
            CancellationToken::new(),
        )?;
        let records = InputResolver::new(&query, None);
        let mut conn = self
            .state
            .registry_tenant_conn(caller.data_tenant_id)
            .await?;
        let status = self
            .queue
            .execution_status(&mut conn, execution_id)
            .await
            .map_err(registry_db_error)?;
        let Some(status) = status else {
            drop(conn);
            return self
                .direct_execution(caller, &records, execution_id)
                .await?
                .ok_or_else(|| WyrdError::VerificationExecutionNotFound {
                    message: "no verification execution with this ID in the caller's tenant"
                        .to_owned(),
                    details: serde_json::json!({ "execution_id": execution_id }),
                });
        };
        let mut state = status.state;
        let verifier = Self::card(&mut conn, &status.verifier_card_uid).await?;
        let subject = Self::card(&mut conn, &status.card_uid).await?;
        drop(conn);
        if state.group_id.is_some() && state.owner_card_uid.is_some() {
            state.group = match records.record(execution_id).await? {
                Some(record) => record.group()?.map(|selector| selector.snapshot()),
                None => None,
            };
        }
        let (Some(result_id), Some(verifier), Some(subject)) =
            (status.result_id, verifier, subject)
        else {
            return Ok(state);
        };
        state.result =
            records
                .result(result_id)
                .await?
                .map(|(run_id, outcome)| VerificationResult {
                    execution_id,
                    run_id,
                    verifier: exact_ref(&verifier),
                    subject: exact_ref(&subject),
                    outcome,
                    deployment_environment: status.deployment_environment,
                });
        Ok(state)
    }

    /// The completed status of direct execution `execution_id`, `None` until
    /// its result, output, and input records are visible or when no direct
    /// execution has this ID. A caller who may not query the verification
    /// tables sees no direct execution, so the lookup answers `None` rather
    /// than refusing an ID that may belong to a queued execution.
    ///
    /// # Errors
    /// Returns a query failure, a registry error when a Card read fails, and
    /// an internal error when a stored record is malformed.
    async fn direct_execution(
        &self,
        caller: &Caller,
        records: &InputResolver<'_>,
        execution_id: ExecutionId,
    ) -> Result<Option<VerificationExecutionState>, WyrdError> {
        let direct = match records.direct(execution_id).await {
            Err(WyrdError::PermissionDeniedRbac { .. }) => return Ok(None),
            read => read?,
        };
        let Some(direct) = direct else {
            return Ok(None);
        };
        let Some((run_id, outcome)) = records.result(direct.result_id).await? else {
            return Ok(None);
        };
        let mut conn = self
            .state
            .registry_tenant_conn(caller.data_tenant_id)
            .await?;
        let verifier = Self::card(&mut conn, &direct.verifier_card_uid).await?;
        let subject = Self::card(&mut conn, &direct.card_uid).await?;
        drop(conn);
        let (Some(verifier), Some(subject)) = (verifier, subject) else {
            return Ok(None);
        };
        Ok(Some(VerificationExecutionState {
            execution_id,
            status: VerificationExecutionStatus::Completed,
            error: None,
            result: Some(VerificationResult {
                execution_id,
                run_id,
                verifier: exact_ref(&verifier),
                subject: exact_ref(&subject),
                outcome,
                deployment_environment: direct.deployment_environment,
            }),
            dispatches: Vec::new(),
            group_id: direct.group_id,
            owner_card_uid: None,
            group: None,
        }))
    }

    /// Plan uploads of the caller's evidence files that are not completed
    /// File uploads.
    ///
    /// Admits and audits any `verifier:run` grant, refuses a file or request
    /// larger than the Deployment limits, and opens one shared storage upload
    /// session for each digest the tenant's inventory holds no completed File
    /// upload of that size for. A Card file of identical content does not
    /// qualify. The bytes go straight to tenant storage through the shared
    /// lifecycle, whose completion records the File in the inventory.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] without any `verifier:run` grant,
    /// [`WyrdError::VerificationInputTooLarge`] past a limit,
    /// [`WyrdError::ServiceUnavailable`] when this process hosts no Verifier
    /// blocks, `WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE` when the
    /// inventory cannot be read now, and the storage error when an upload
    /// cannot be initialized.
    pub(crate) async fn evidence(
        &self,
        caller: &Caller,
        request: &EvidencePlanRequest,
    ) -> Result<EvidencePlanResponse, WyrdError> {
        let limits = *self.blocks()?.limits();
        self.authorize_evidence(caller, EVIDENCE, "verification.evidence")?;
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
        let inventory = ObjectInventory::new(self.state);
        let mut uploads = Vec::new();
        for file in &request.files {
            if inventory
                .file_object(caller.data_tenant_id, &file.sha256, file.size_bytes)
                .await?
                .is_some()
            {
                continue;
            }
            let init = wyrd_storage::service::file_upload_init(
                &self.state.storage,
                self.state.postgres.wyrd(),
                &storage_caller(caller),
                &file.sha256,
                file.size_bytes,
            )
            .await?;
            uploads.push(EvidenceUpload {
                sha256: file.sha256,
                upload_id: init.upload_id,
                plan: init.plan,
            });
        }
        Ok(EvidencePlanResponse { uploads })
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
    /// engine; MCP passes `None`. The execution's profile starts at the HTTP
    /// `arrival` (or now, for MCP) and is emitted once under the outcome
    /// label after the result is staged, so its overhead covers admission,
    /// load, engine bookkeeping, and staging less the Verifier blocks.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] without `verifier:run` on
    /// the Verifier or when a Card-scoped caller's signed scope does not
    /// carry the subject, [`WyrdError::WorkloadContextRequired`] when a
    /// family-restricted caller selected no root,
    /// [`WyrdError::VerificationTargetNotFound`] for an
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
    /// unavailability error when a read fails, the Verifier's planning
    /// refusal, and [`WyrdError::VerificationStagingFull`] when the Scribe
    /// outbox cannot reserve the inputs and result slots, before anything
    /// is staged or the engine runs.
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
        arrival: Option<RequestArrival>,
    ) -> Result<VerificationResult, WyrdError> {
        let telemetry =
            ExecutionTelemetry::start(ExecutionMode::Direct, arrival.map(RequestArrival::instant));
        let (label, result) = self
            .run_direct(caller, request, edge_timer, &telemetry)
            .await;
        tracing::Span::current().record("outcome", label);
        telemetry.finish(label, label != "completed");
        result
    }

    /// The body of [`execute`](Self::execute), timed by `telemetry`: admit,
    /// load, run the engine, and stage. Returns the outcome label the
    /// profile is emitted under beside the result; a request refused before
    /// the engine ran is `refused`, or `unavailable` when this process hosts
    /// no Verifier blocks.
    ///
    /// # Errors
    /// The returned result carries every refusal [`execute`](Self::execute)
    /// documents: subject admission and staging capacity before the target
    /// loads, then target, environment, engine, deadline, and staging
    /// failures, each paired with its outcome label.
    async fn run_direct(
        &self,
        caller: &Caller,
        request: &VerificationRequest,
        edge_timer: Option<&EdgeTimer>,
        telemetry: &ExecutionTelemetry,
    ) -> (&'static str, Result<VerificationResult, WyrdError>) {
        let blocks = match self.blocks() {
            Ok(blocks) => blocks,
            Err(error) => return ("unavailable", Err(error)),
        };
        let execution_id = ExecutionId::new_v7();
        tracing::Span::current().record("execution_id", tracing::field::display(execution_id));
        let resource = format!(
            "verifier:{}/subject:{}/execution:{execution_id}",
            request.verifier_card_uid, request.card_uid
        );
        let admitted = telemetry.time(Phase::Admit, || {
            environment_tag(request)?;
            self.authorize_run(
                caller,
                &request.verifier_card_uid,
                &request.card_uid,
                EXECUTE,
                &resource,
            )?;
            // The inputs and result rows, reserved before any work is done.
            self.state
                .bifrost
                .scribe_outbox()
                .reserve(2)
                .map_err(staging_full)
        });
        let staged = match admitted {
            Ok(staged) => staged,
            Err(error) => return ("refused", Err(error)),
        };
        let target = match telemetry
            .phase(
                Phase::Load,
                self.resolve(caller, request)
                    .instrument(tracing::info_span!("verification.load")),
            )
            .await
        {
            Ok(target) => target,
            Err(error) => return ("refused", Err(error)),
        };
        if let Some(edge_timer) = edge_timer {
            edge_timer.hand_off();
        }
        let Some(system) = target.system_principal else {
            return (
                "errored",
                Err(WyrdError::internal(
                    "the tenant has no active SYSTEM principal to execute the Verifier",
                )),
            );
        };
        let executor = caller.clone().executed_by_system(system);
        let started_at = Utc::now();
        let timeout = blocks.limits().verify_timeout;
        let engine = match blocks.engine(
            self.state,
            Execution {
                caller: &executor,
                evidence: &request.inputs,
                routes: &request.execution_route_overrides,
                subject: GatewayCallContext {
                    workload_card_uid: wyrd_runtime::owning_root(
                        &caller.principal,
                        &caller.delegation_chain,
                    )
                    .cloned(),
                    subject_card_uid: Some(request.card_uid.clone()),
                    run_id: request.run_id.clone(),
                    deployment_environment: request.deployment_environment.clone(),
                },
                defer_unlanded_traces: false,
                drift: DriftChecks::new(None, BTreeMap::new(), BTreeMap::new()),
                clock: telemetry.clock(),
            },
        ) {
            Ok(engine) => engine,
            Err(error) => return ("errored", Err(error)),
        };
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
        let outcome = match ran {
            Ok(outcome) => outcome,
            Err(EngineError::DeadlineExceeded) => {
                return (
                    "timed_out",
                    Err(WyrdError::VerificationExecutionTimedOut {
                        message: "the Verifier exceeded the direct execution deadline".to_owned(),
                        details: serde_json::json!({ "deadline_seconds": timeout.as_secs() }),
                    }),
                );
            }
            Err(EngineError::Plan(error)) => return ("errored", Err(error)),
            Err(EngineError::Busy(reason)) => {
                return (
                    "unavailable",
                    Err(WyrdError::ServiceUnavailable {
                        message: "the verification runtime has no capacity for this Verifier now"
                            .to_owned(),
                        details: serde_json::json!({ "reason": reason }),
                    }),
                );
            }
            Err(EngineError::InputInvalid(reason)) => {
                return (
                    "refused",
                    Err(WyrdError::VerificationInputInvalid {
                        message: reason.clone(),
                        details: serde_json::json!({ "reason": reason }),
                    }),
                );
            }
            Err(EngineError::InputTooLarge(reason)) => {
                return (
                    "refused",
                    Err(WyrdError::VerificationInputTooLarge {
                        message: reason.clone(),
                        details: serde_json::json!({ "reason": reason }),
                    }),
                );
            }
            Err(EngineError::EvidenceUnavailable(reason)) => {
                return (
                    "unavailable",
                    Err(WyrdError::ServiceUnavailable {
                        message: "the verification evidence cannot be admitted now".to_owned(),
                        details: serde_json::json!({ "reason": reason }),
                    }),
                );
            }
            Err(error) => return ("errored", Err(WyrdError::internal(error.to_string()))),
        };
        telemetry.time(Phase::Stage, || {
            Self::stage_direct(
                &staged,
                caller,
                execution_id,
                request,
                &target,
                &outcome,
                started_at,
            );
        });
        (
            "completed",
            Ok(VerificationResult {
                execution_id,
                run_id: request.run_id.clone(),
                verifier: target.verifier.clone(),
                subject: target.subject.clone(),
                outcome,
                deployment_environment: request.deployment_environment.clone(),
            }),
        )
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
    /// past a Variant limit, `WYRD_SPEC_400_VALIDATION` for an empty
    /// `deployment_environment`, [`WyrdError::ServiceUnavailable`] when this
    /// process hosts no Verifier queue, and
    /// [`WyrdError::VerificationStagingFull`] when the submission or Scribe
    /// outbox cannot reserve its slot, before any input is staged.
    pub(crate) fn enqueue(
        &self,
        caller: &Caller,
        request: VerificationRequest,
        key: Option<&IdempotencyKey>,
    ) -> Result<EnqueueVerificationResponse, WyrdError> {
        let submissions = self
            .state
            .bifrost
            .scribe_outbox()
            .sink()
            .reaches_scribe()
            .then_some(self.state.verification.submissions())
            .ok_or_else(Self::unavailable)?;
        let resource = format!(
            "verifier:{}/subject:{}",
            request.verifier_card_uid, request.card_uid
        );
        self.authorize_run(
            caller,
            &request.verifier_card_uid,
            &request.card_uid,
            ENQUEUE,
            &resource,
        )?;
        environment_tag(&request)?;
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
                owner_card_uid: wyrd_runtime::owning_root(
                    &caller.principal,
                    &caller.delegation_chain,
                ),
                group_id: submission.request.group_id.as_ref(),
                deployment_environment: submission.request.deployment_environment.as_deref(),
                binding_id: None,
                trigger: None,
                window: None,
            },
            &submission.request.verifier_card_uid,
            now,
            now,
        )
        .inputs(InputRecord::Submitted(&submission))
        .map_err(|error| payload_error(&error))?;
        // The staged row is the one store of the submission's values; the
        // acknowledged handoff carries only its coordination identity.
        let mut submission = submission;
        std::mem::take(&mut submission.request.inputs);
        std::mem::take(&mut submission.request.execution_route_overrides);
        let queued = submissions.reserve(1).map_err(staging_full)?;
        let staged = self
            .state
            .bifrost
            .scribe_outbox()
            .reserve(1)
            .map_err(staging_full)?;
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
    /// as `subject_card_uid` and the exact Verifier as `verifier_card_uid`, are
    /// attributed to the caller, carry the caller's application Run, group,
    /// and authenticated owner Service, and have no binding or Trigger. The inputs row never queues work. The
    /// verdict is already decided, so an unbuildable payload is logged and
    /// not recorded.
    fn stage_direct(
        staged: &Reservation<ScribeSink>,
        caller: &Caller,
        execution_id: ExecutionId,
        request: &VerificationRequest,
        target: &Target,
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
                owner_card_uid: wyrd_runtime::owning_root(
                    &caller.principal,
                    &caller.delegation_chain,
                ),
                group_id: request.group_id.as_ref(),
                deployment_environment: request.deployment_environment.as_deref(),
                binding_id: None,
                trigger: None,
                window: None,
            },
            &request.verifier_card_uid,
            started_at,
            Utc::now(),
        );
        let built = builder
            .inputs(InputRecord::Direct(&request.inputs))
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

    /// Resolve a direct execution's target through the one read route every
    /// Verifier execution shares,
    /// [`VerifierCardCache::resolve`](crate::verification::cache::VerifierCardCache::resolve).
    ///
    /// A cached target costs no IO; a miss reads both Cards, the Verifier's
    /// readiness, and the tenant SYSTEM principal in one autocommit statement
    /// that binds the caller's tenant for RLS, so a Card of another tenant is
    /// invisible. A drift Verifier judges a scheduled window and cannot
    /// verify now, whether or not its baseline is ready.
    ///
    /// # Errors
    /// Returns a registry error when the read fails,
    /// [`WyrdError::VerificationInputUnsupported`] for a Window Verifier, and
    /// [`WyrdError::VerificationTargetNotFound`] for any other refused target.
    async fn resolve(
        &self,
        caller: &Caller,
        request: &VerificationRequest,
    ) -> Result<Arc<Target>, WyrdError> {
        let window = || WyrdError::VerificationInputUnsupported {
            message: "a drift Verifier judges a scheduled window; record samples and submit \
                      them instead of verifying now"
                .to_owned(),
            details: serde_json::json!({ "verifier_card_uid": request.verifier_card_uid }),
        };
        let resolved = self
            .state
            .verification
            .verifier_cards()
            .resolve(
                Db::Pool(self.state.postgres.wyrd()),
                caller.data_tenant_id,
                &request.verifier_card_uid,
                &request.card_uid,
            )
            .await
            .map_err(registry_db_error)?;
        let target = match resolved {
            Ok(target) => target,
            Err(EnqueueRefusal::NotReady(VerifierReadiness::BaselineNotReady)) => {
                return Err(window());
            }
            Err(_) => {
                return Err(WyrdError::VerificationTargetNotFound {
                    message:
                        "no active Verifier and subject Card with these UIDs in the caller's tenant"
                            .to_owned(),
                    details: serde_json::json!({
                        "verifier_card_uid": request.verifier_card_uid,
                        "card_uid": request.card_uid,
                    }),
                });
            }
        };
        if target
            .spec
            .all_requirements()
            .any(|requirement| requirement.check.evidence() == CheckEvidence::Window)
        {
            return Err(window());
        }
        Ok(target)
    }

    /// The process's hosted Verifier blocks.
    ///
    /// # Errors
    /// Returns [`WyrdError::ServiceUnavailable`] when this process composed
    /// none.
    fn blocks(&self) -> Result<&HostedBlocks, WyrdError> {
        self.state
            .verification
            .blocks()
            .map(Arc::as_ref)
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
    /// Direct and queued work both admit through here, before anything is
    /// reserved or staged. A Card-scoped caller must carry the verified
    /// `subject` in its signed Card scope, and a family-restricted caller
    /// must have selected a root; this is the same in-memory rule the
    /// gateway applies to attribution, so no registry read is needed. A
    /// caller that may attribute to any registered Card is decided by RBAC
    /// alone.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] when RBAC refuses the
    /// Verifier or the signed scope does not carry `subject`, and
    /// [`WyrdError::WorkloadContextRequired`] when a family-restricted
    /// caller selected no root.
    fn authorize_run(
        &self,
        caller: &Caller,
        verifier: &CardUid,
        subject: &CardUid,
        operation: &str,
        resource: &str,
    ) -> Result<(), WyrdError> {
        let required = Permission::verifier_run(verifier.clone());
        let decision = self
            .state
            .auth
            .permission_check
            .check(&caller.principal, &required)
            .into_result()
            .map_err(permission_deny_reason_to_wyrd)
            .and_then(|()| match caller.principal.card_attribution() {
                CardAttribution::RootRequired(_) => Err(WyrdError::workload_context_required(
                    "the caller is restricted to one workload family and selected no root",
                )),
                CardAttribution::Scoped(scope) if !scope.contains(subject) => {
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
        self.state.bifrost.scribe_outbox().stage(
            caller.data_tenant_id,
            audit::audit_event(caller, operation, resource, &required.to_string(), outcome),
        );
        decision
    }

    /// Admit evidence staging for `operation` on `resource` and stage the
    /// single allow or deny decision without waiting for its commit.
    ///
    /// Staging selects no Verifier and cannot execute one, so any
    /// `verifier:run` grant admits it, including one scoped to a single
    /// Verifier UID. Planning and every shared storage operation on a File
    /// upload session are admitted this way. The exact Verifier is
    /// authorized later by [`Self::authorize_run`] when execute or enqueue
    /// names it.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] when the caller holds no
    /// `verifier:run` grant of any scope.
    pub(crate) fn authorize_evidence(
        &self,
        caller: &Caller,
        operation: &str,
        resource: &str,
    ) -> Result<(), WyrdError> {
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
        self.state.bifrost.scribe_outbox().stage(
            caller.data_tenant_id,
            audit::audit_event(caller, operation, resource, &required.to_string(), outcome),
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
