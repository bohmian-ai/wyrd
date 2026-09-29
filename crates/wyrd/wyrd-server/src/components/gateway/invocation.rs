//! Governed gateway invocation shared by public handlers and trusted
//! server-internal callers.
//!
//! [`GatewayInvocation::invoke`] is the single pipeline: validate the request,
//! authorize and audit the exact requested model, load one immutable tenant
//! snapshot, plan candidates, authorize and audit every fallback candidate,
//! admit against limits and budgets in tenant Postgres, execute through the
//! shared [`GatewayEngine`], and account every attempt. Identity comes only
//! from the verified [`Caller`]; the gateway engine never sees a database
//! handle.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use serde_json::value::to_raw_value;
use serde_json::{Value, json};
use tokio::sync::oneshot;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wyrd_gateway::{
    AttemptResult, AttemptUsage, BatchAction, CallExecution, CallInput, CallPlan,
    CredentialResolver, DeploymentHealth, FailureClass, GatewayEngine, GatewayTenantSnapshot,
    IngressDialect, MediaRequest, ProviderAttempt, ProviderDispatch, ResponseBody, ResponseCapture,
    StreamEnd, outcome_error_code, outcome_name,
};
use wyrd_runtime::{Permission, PermissionDenyReason, Principal};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::GatewayAccess;
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::{
    GatewayAccountingEntryV1, GatewayCallId, GatewayCallOutcome, GatewayCaptureMode,
    GatewayContractError, GatewayFallbackOverride, GatewayOperation, GatewayPayloadField,
    GatewayUsageAmount, ModelRef,
};
use wyrd_spec::ids::ProviderDeploymentName;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::AuditOutcome;

use super::capture::{
    CallCapture, CallFacts, CaptureDrop, GatewayCapture, PayloadObjects, error_code,
    operation_name, request_content, selects,
};
use super::ledger::{Admission, GatewayLedger, LedgerCall};
use super::service::{GatewayAdministration, unavailable};
use crate::audit;
use crate::components::auth::Caller;
use crate::http::error::permission_deny_reason_to_wyrd;
use crate::state::AppState;

/// Audited operation name of every invocation decision.
pub(crate) const OPERATION: &str = "gateway.invoke";

/// Time past the caller deadline before an unsettled reservation or lease may
/// be reclaimed by another admission.
const RECLAIM_GRACE: TimeDelta = TimeDelta::minutes(1);

/// One governed call as a verified caller submits it.
#[derive(Debug, Clone)]
pub struct GatewayCallRequest {
    /// Requested operation.
    pub operation: GatewayOperation,
    /// Dialect of `body`: `OpenAI` for public routes, or the provider-native
    /// dialect a Skald caller produced.
    pub ingress: IngressDialect,
    /// Exact requested model.
    pub model: ModelRef,
    /// Per-request fallback override replacing every tenant rule.
    pub fallback: Option<GatewayFallbackOverride>,
    /// Operation request body handed to the provider attempt.
    pub body: Value,
    /// Route and uploaded files of an Images or Audio call.
    pub media: Option<MediaRequest>,
    /// Lifecycle action of a Batches call.
    pub batch: Option<BatchAction>,
    /// Deployment the call must reach because it holds the provider state the
    /// call continues; `None` routes normally.
    pub deployment: Option<ProviderDeploymentName>,
    /// Whether the caller asked for an incremental server-sent event answer.
    pub stream: bool,
    /// Largest usage the call can incur, or `None` when unbounded.
    pub usage_bound: Option<Vec<GatewayUsageAmount>>,
    /// Caller deadline measured from admission.
    pub timeout: Duration,
}

/// Provider answer of a governed call: a completion, or the refusal that
/// ended the call.
#[derive(Debug)]
pub struct GatewayCallResponse {
    /// Logical call identity recorded in the ledger.
    pub call_id: GatewayCallId,
    /// Model the caller requested.
    pub requested: ModelRef,
    /// Model that produced the response.
    pub resolved: ModelRef,
    /// Deployment of the last attempt, which produced the response.
    pub deployment: Option<ProviderDeploymentName>,
    /// Provider HTTP status: `200` for a completion, otherwise the refusing
    /// provider's status.
    pub status: u16,
    /// Operation response bytes or event stream, or the refusal rendered in
    /// the caller's dialect.
    pub body: ResponseBody,
}

/// A governed call that ended in an error, with whether a provider may have
/// received it.
///
/// Callers that fence non-idempotent provider work keep the fence only when
/// `dispatched` is set.
#[derive(Debug)]
pub(crate) struct CallFailure {
    /// Error returned to the caller.
    pub(crate) error: WyrdError,
    /// Whether a provider may have received the request: an attempt began
    /// dispatch, or the call's outcome can no longer be observed.
    pub(crate) dispatched: bool,
    /// Whether the execution task already recorded the call's terminal
    /// `outcome` on its `gateway.call` span; otherwise [`GatewayInvocation::run`]
    /// records it.
    pub(crate) settled: bool,
}

impl From<WyrdError> for CallFailure {
    /// Marks an error raised before any attempt ran as undispatched.
    fn from(error: WyrdError) -> Self {
        Self {
            error,
            dispatched: false,
            settled: false,
        }
    }
}

impl From<CallFailure> for WyrdError {
    /// Drops the dispatch fact for callers that only report the error.
    fn from(failure: CallFailure) -> Self {
        failure.error
    }
}

/// Dependency-owning handle for governed gateway calls.
pub struct GatewayInvocation<'a> {
    /// Server state carrying authorization, audit, storage, and the engine.
    state: &'a AppState,
}

/// Everything admission fixed for one call, owned so execution and accounting
/// can outlive the caller's future.
struct AdmittedCall {
    /// Logical call identity.
    call_id: GatewayCallId,
    /// Verified tenant.
    tenant: DataTenantId,
    /// Verified caller.
    principal: Principal,
    /// Request that admitted the call, carried into its capture.
    request_id: RequestId,
    /// Immutable admitted configuration.
    snapshot: GatewayTenantSnapshot,
    /// Operation request body.
    body: Value,
    /// Route and uploaded files of an Images or Audio call.
    media: Option<MediaRequest>,
    /// Lifecycle action of a Batches call.
    batch: Option<BatchAction>,
    /// Usage bound used for budget reservation.
    usage_bound: Option<Vec<GatewayUsageAmount>>,
    /// Admission time.
    admitted_at: DateTime<Utc>,
    /// Reclaim time of reservations and leases.
    expires_at: DateTime<Utc>,
    /// Absolute caller deadline.
    deadline: Instant,
    /// Model the caller requested.
    requested: ModelRef,
}

impl AdmittedCall {
    /// Borrows the ledger's view of this call.
    fn ledger(&self) -> LedgerCall<'_> {
        LedgerCall {
            call_id: self.call_id,
            tenant: self.tenant,
            principal: &self.principal,
            governance: &self.snapshot.governance,
            usage_bound: self.usage_bound.as_deref(),
            admitted_at: self.admitted_at,
            expires_at: self.expires_at,
        }
    }
}

impl<'a> GatewayInvocation<'a> {
    /// Borrows invocation dependencies from server state.
    #[must_use]
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// Runs one governed call for `caller`.
    ///
    /// Authorization of the requested model precedes every other step; each
    /// candidate decision is audited standalone. Admission commits before any
    /// credential or provider work. Execution and accounting run in a task on
    /// the server's gateway tracker, so dropping this future cancels the
    /// in-flight attempt yet still records its accounting, and shutdown drain
    /// waits for that accounting.
    ///
    /// A buffered answer returns once its accounting commits. A streamed
    /// answer returns as soon as the stream opens; once the relay reports the
    /// stream's end, the task settles the call and its completing attempt to
    /// that terminal outcome, time, and usage before accounting, metrics, and
    /// capture, and logs an accounting failure it can no longer return.
    ///
    /// A call whose last attempt the provider refused returns that refusal's
    /// status and body rather than an error, so native callers keep provider
    /// error semantics.
    ///
    /// Once the server drains, new calls are refused before authorization.
    /// Dropping this future before the answer cancels the call; once the
    /// answer is returned, a streamed body ends with its terminal error on
    /// drain or deadline instead.
    ///
    /// # Errors
    /// A `deployment` pin restricts the plan to that deployment of the requested
    /// model; a pin no capable deployment matches refuses the call as
    /// unavailable.
    ///
    /// Returns `ServiceUnavailable` while the server drains,
    /// `GatewayInvalidRequest` for an invalid override or timeout or
    /// a request no authorized deployment can represent, the mapped
    /// permission denial for the requested model, `GatewayModelUnavailable` when no
    /// authorized capable candidate exists, the admission rejections of
    /// [`GatewayLedger::admit`], `GatewayDeadlineExceeded` on deadline expiry,
    /// `GatewayUpstreamUnavailable` when every attempt fails, and
    /// `ServiceUnavailable` on storage failure, cancellation, or drain.
    pub async fn invoke(
        &self,
        caller: &Caller,
        request: GatewayCallRequest,
    ) -> Result<GatewayCallResponse, WyrdError> {
        Ok(self
            .run(caller, request, false, &self.state.shutdown_token)
            .await?)
    }

    /// Runs one governed call exactly as [`Self::invoke`] does, reporting with
    /// each error whether a provider may have received the request.
    ///
    /// `authorized` skips the invoke decision when the caller already took and
    /// audited it for the requested model. The call's cancellation is a child
    /// of `cancel`, so an owner that keeps this future alive can still cancel
    /// the call and observe its dispatch evidence; `invoke` passes the server
    /// shutdown token.
    ///
    /// Every error raised before execution starts (drain, validation,
    /// authorization, snapshot, representability, admission) is undispatched.
    /// After execution the call is dispatched when any attempt is billable,
    /// and also when its task ended without answering, since its outcome is
    /// then unknown. Dropping this future after dispatch cancels the call
    /// without reporting.
    ///
    /// Every undispatched failure is counted under its stable error code, which
    /// covers drain, validation, authorization, rate-limit, and budget denials.
    ///
    /// Traced as one `gateway.call` span, tagged with the caller's request ID,
    /// whose authorization, routing,
    /// admission, execution, attempt, translation, accounting, and capture
    /// spans are its descendants. Every call closes with one terminal
    /// `outcome` and, unless it succeeded, a stable `error.code`. The
    /// execution task records the outcome of an executed call; a call refused
    /// before execution, or whose task ended without answering, is recorded
    /// here as `Cancelled` when drain or cancellation made the service
    /// unavailable and `Failed` otherwise. The code is recorded here for a
    /// returned error, or by the execution task for a non-successful call the
    /// caller received as an answer.
    ///
    /// # Errors
    /// Returns every error of [`Self::invoke`] as a [`CallFailure`].
    #[tracing::instrument(
        name = "gateway.call",
        skip_all,
        fields(
            operation = operation_name(request.operation),
            request_id = %caller.request_id,
            call_id = tracing::field::Empty,
            outcome = tracing::field::Empty,
            error.code = tracing::field::Empty,
        )
    )]
    pub(crate) async fn run(
        &self,
        caller: &Caller,
        request: GatewayCallRequest,
        authorized: bool,
        cancel: &CancellationToken,
    ) -> Result<GatewayCallResponse, CallFailure> {
        let operation = operation_name(request.operation);
        let result = self.govern(caller, request, authorized, cancel).await;
        if let Err(failure) = &result {
            let span = tracing::Span::current();
            if !failure.settled {
                let stopped = matches!(failure.error, WyrdError::ServiceUnavailable { .. })
                    && (self.state.shutdown_token.is_cancelled() || cancel.is_cancelled());
                let outcome = if stopped {
                    GatewayCallOutcome::Cancelled
                } else {
                    GatewayCallOutcome::Failed
                };
                span.record("outcome", outcome_name(outcome));
            }
            span.record("error.code", failure.error.code());
            if !failure.dispatched {
                metrics::counter!(
                    "wyrd_gateway_refusals_total",
                    "operation" => operation,
                    "code" => failure.error.code()
                )
                .increment(1);
            }
        }
        result
    }

    /// Runs one gateway call: admits it on the caller's task, then executes
    /// and settles it on a tracked background [`CallTask`] so accounting and
    /// capture still finish if the caller goes away.
    ///
    /// Latency: the caller waits for admission (see [`Self::admit_call`]),
    /// the provider attempts, and, for a buffered answer, the accounting
    /// commit. A stream answers when it opens. Capture never delays the
    /// answer. Dropping this future before the answer cancels the call.
    ///
    /// # Errors
    /// Returns every error of [`Self::invoke`] as a [`CallFailure`]. Errors
    /// from [`Self::admit_call`] are undispatched, and a task that ends
    /// without answering is dispatched but unsettled.
    async fn govern(
        &self,
        caller: &Caller,
        request: GatewayCallRequest,
        authorized: bool,
        cancel: &CancellationToken,
    ) -> Result<GatewayCallResponse, CallFailure> {
        let received = Instant::now();
        let (ingress, stream) = (request.ingress, request.stream);
        let (call, plan, admission) = self.admit_call(caller, request, authorized).await?;
        let cancel = cancel.child_token();
        let cancel_on_drop = cancel.clone().drop_guard();
        let (head, answer) = oneshot::channel();
        let task = CallTask {
            state: self.state.clone(),
            call,
            plan,
            admission,
            ingress,
            stream,
            received,
            call_span: tracing::Span::current(),
            cancel,
        };
        self.state.gateway_tasks.spawn(
            task.run(head)
                .instrument(tracing::info_span!("gateway.execute")),
        );
        let (answer, dispatched) = answer.await.map_err(|error| CallFailure {
            error: unavailable(error),
            dispatched: true,
            settled: false,
        })?;
        // The answer reached the caller: a stream now ends on drain, deadline,
        // or its own drop rather than when this future returns.
        cancel_on_drop.disarm();
        answer.map_err(|error| CallFailure {
            error,
            dispatched,
            settled: true,
        })
    }

    /// Checks, authorizes, routes, and reserves one call before any provider
    /// sees it.
    ///
    /// Latency: on the request path, with sequential Postgres round trips:
    /// the tenant snapshot load and the reservation commit in
    /// [`Self::admit`]. Every invoke-decision audit, here and per fallback
    /// model in [`Self::route`], is staged on `gateway_tasks` and is not a
    /// request-path round trip.
    ///
    /// # Errors
    /// Returns `ServiceUnavailable` while the server drains,
    /// `GatewayInvalidRequest` for an invalid fallback override or
    /// out-of-range timeout, the mapped permission denial of the requested
    /// model, and every error of the snapshot load, [`Self::route`], and
    /// [`Self::admit`]. Every one is undispatched.
    async fn admit_call(
        &self,
        caller: &Caller,
        request: GatewayCallRequest,
        authorized: bool,
    ) -> Result<(AdmittedCall, CallPlan, Admission), CallFailure> {
        let call_id = GatewayCallId::new_v7();
        tracing::Span::current().record("call_id", tracing::field::display(call_id.as_uuid()));
        if self.state.shutdown_token.is_cancelled() {
            return Err(WyrdError::ServiceUnavailable {
                message: "the server is draining and admits no new gateway call".to_owned(),
                details: json!({}),
            }
            .into());
        }
        if let Some(fallback) = &request.fallback {
            fallback
                .validate_for(&request.model)
                .map_err(invalid_request)?;
        }
        let grace = TimeDelta::from_std(request.timeout)
            .ok()
            .and_then(|timeout| timeout.checked_add(&RECLAIM_GRACE))
            .ok_or_else(|| {
                invalid_request(GatewayContractError::new("timeout", "is out of range"))
            })?;
        if !authorized {
            self.decide(caller, OPERATION, &request.model)
                .map_err(permission_deny_reason_to_wyrd)?;
        }

        let tenant = caller.data_tenant_id;
        let snapshot = GatewayAdministration::new(self.state)
            .snapshot(tenant)
            .await?;
        let mut plan = self.route(caller, &request, &snapshot, call_id).await?;

        let admitted_at = Utc::now();
        let call = AdmittedCall {
            call_id,
            tenant,
            principal: caller.principal.clone(),
            request_id: caller.request_id.clone(),
            snapshot,
            body: request.body,
            media: request.media,
            batch: request.batch,
            usage_bound: request.usage_bound,
            admitted_at,
            expires_at: admitted_at + grace,
            deadline: Instant::now() + request.timeout,
            requested: request.model,
        };
        let admission = self.admit(&call, &mut plan).await?;
        Ok((call, plan, admission))
    }

    /// Plans the candidate routes of one call inside a `gateway.route` span.
    ///
    /// Builds the fallback plan from the admitted `snapshot`, applies a pinned
    /// deployment, authorizes every fallback model other than the requested
    /// one, and keeps only candidates that can represent the request.
    ///
    /// # Errors
    /// Returns `GatewayInvalidRequest` when no authorized candidate can
    /// represent the request and `GatewayModelUnavailable` when no authorized
    /// candidate remains. An authorization decision returns no error of its
    /// own: its audit is staged off the request path, so a failed append is
    /// logged and counted rather than surfaced here.
    #[tracing::instrument(name = "gateway.route", skip_all)]
    async fn route(
        &self,
        caller: &Caller,
        request: &GatewayCallRequest,
        snapshot: &GatewayTenantSnapshot,
        call_id: GatewayCallId,
    ) -> Result<CallPlan, CallFailure> {
        let mut plan = CallPlan::new(
            snapshot,
            call_id,
            request.operation,
            request.model.clone(),
            request.fallback.as_ref(),
        );
        if let Some(deployment) = &request.deployment {
            plan.pin(deployment);
        }
        let mut denied = Vec::new();
        for candidate in &plan.candidates {
            if candidate.model != request.model
                && self.decide(caller, OPERATION, &candidate.model).is_err()
            {
                denied.push(candidate.model.clone());
            }
        }
        plan.retain(|model| !denied.contains(model));
        let unsupported = plan
            .retain_representable(
                request.ingress,
                request.stream,
                &request.body,
                request.media.as_ref(),
            )
            .err();
        if plan.candidates.is_empty() {
            return Err(CallFailure::from(match unsupported {
                Some(error) => WyrdError::GatewayInvalidRequest {
                    message: error.to_string(),
                    details: json!({ "field": error.field, "reason": error.reason }),
                },
                None => WyrdError::GatewayModelUnavailable {
                    message: "no authorized deployment serves the requested operation".to_owned(),
                    details: json!({}),
                },
            }));
        }
        Ok(plan)
    }

    /// Maps a terminal `execution` of `call` to the caller's answer.
    ///
    /// # Errors
    /// Returns `GatewayDeadlineExceeded` for a timed-out call,
    /// `ServiceUnavailable` for a cancelled call or a refusal body that cannot
    /// be serialized, and `GatewayUpstreamUnavailable` when no attempt
    /// completed.
    fn respond(
        call: &AdmittedCall,
        execution: CallExecution,
    ) -> Result<GatewayCallResponse, WyrdError> {
        let call_id = call.call_id;
        let (last_model, deployment) = execution
            .attempts
            .last()
            .map(|attempt| (attempt.model.clone(), attempt.deployment.clone()))
            .unzip();
        match (
            execution.outcome,
            execution.response,
            execution.resolved,
            execution.refusal,
        ) {
            (GatewayCallOutcome::Succeeded, Some(body), Some(resolved), _) => {
                Ok(GatewayCallResponse {
                    call_id,
                    requested: call.requested.clone(),
                    resolved,
                    deployment,
                    status: 200,
                    body,
                })
            }
            (GatewayCallOutcome::Failed, _, _, Some(refusal)) => Ok(GatewayCallResponse {
                call_id,
                resolved: last_model.unwrap_or_else(|| call.requested.clone()),
                requested: call.requested.clone(),
                deployment,
                status: refusal.status,
                body: ResponseBody::Json(to_raw_value(&refusal.body).map_err(unavailable)?),
            }),
            (GatewayCallOutcome::TimedOut, ..) => Err(WyrdError::GatewayDeadlineExceeded {
                message: "the gateway call exceeded its deadline".to_owned(),
                details: json!({ "call_id": call_id }),
            }),
            (GatewayCallOutcome::Cancelled, ..) => Err(WyrdError::ServiceUnavailable {
                message: "the gateway call was cancelled".to_owned(),
                details: json!({ "call_id": call_id }),
            }),
            _ => Err(WyrdError::GatewayUpstreamUnavailable {
                message: "no provider attempt completed the gateway call".to_owned(),
                details: json!({ "call_id": call_id }),
            }),
        }
    }

    /// Lists the exact models configured for the caller's tenant that the
    /// caller may invoke, sorted by projection.
    ///
    /// Every distinct deployment model is checked against invoke permission
    /// and each decision is audited under `gateway.models.list`; a denied
    /// model is omitted rather than refused.
    ///
    /// # Errors
    /// Returns `ServiceUnavailable` while the server drains or when the tenant
    /// snapshot cannot be loaded.
    pub async fn models(&self, caller: &Caller) -> Result<Vec<ModelRef>, WyrdError> {
        if self.state.shutdown_token.is_cancelled() {
            return Err(WyrdError::ServiceUnavailable {
                message: "the server is draining and admits no new gateway call".to_owned(),
                details: json!({}),
            });
        }
        let snapshot = GatewayAdministration::new(self.state)
            .snapshot(caller.data_tenant_id)
            .await?;
        let configured: BTreeSet<ModelRef> = snapshot
            .deployments
            .into_iter()
            .map(|deployment| deployment.model)
            .collect();
        let mut visible = Vec::with_capacity(configured.len());
        for model in configured {
            if self.decide(caller, "gateway.models.list", &model).is_ok() {
                visible.push(model);
            }
        }
        Ok(visible)
    }

    /// Checks invoke permission for exactly `model` and stages the decision
    /// under the audited `operation`.
    ///
    /// Returns the verdict; the caller decides whether a denial refuses the
    /// call (requested model), only skips a candidate (fallback), or hides the
    /// model from a listing. The decision row is appended by a task spawned on
    /// [`AppState::gateway_tasks`] rather than awaited here, so no invocation
    /// waits on Postgres to record its own decision and shutdown still drains
    /// the append. A staged append that fails is logged and counted under
    /// `gateway_audit_commit_failures_total`; that decision keeps no row, and
    /// an abrupt process loss may drop appends that had not yet committed.
    ///
    /// # Errors
    /// Returns the deny reason when `caller` may not invoke `model`.
    #[tracing::instrument(name = "gateway.authorize", skip_all, fields(operation))]
    pub(crate) fn decide(
        &self,
        caller: &Caller,
        operation: &str,
        model: &ModelRef,
    ) -> Result<(), PermissionDenyReason> {
        let permission = Permission::gateway_invoke(GatewayAccess::Model {
            provider: model.provider.clone(),
            model: model.model.clone(),
        });
        let verdict = self
            .state
            .authz
            .permission_check
            .check(&caller.principal, &permission)
            .into_result();
        let outcome = if verdict.is_ok() {
            AuditOutcome::Allowed
        } else {
            AuditOutcome::Denied
        };
        let resource = format!(
            "gateway_model:{}/{}",
            model.provider.as_str(),
            model.model.as_str()
        );
        let event = audit::audit_event(
            caller,
            operation,
            &resource,
            &permission.to_string(),
            outcome,
        );
        let pool = self.state.postgres.vala_pool().clone();
        let tenant = caller.data_tenant_id;
        self.state.gateway_tasks.spawn(async move {
            if let Err(error) = audit::record_audit(&pool, tenant, &event).await {
                metrics::counter!("gateway_audit_commit_failures_total").increment(1);
                tracing::error!(
                    %error,
                    operation = %event.operation,
                    request_id = %event.request_id,
                    "gateway invoke decision did not commit to the audit outbox"
                );
            }
        });
        verdict
    }

    /// Admits `call` in its own committed tenant transaction.
    ///
    /// # Errors
    /// Returns the admission rejections of [`GatewayLedger::admit`] and
    /// `ServiceUnavailable` when storage fails; a rejection commits nothing.
    #[tracing::instrument(name = "gateway.admit", skip_all)]
    async fn admit(
        &self,
        call: &AdmittedCall,
        plan: &mut CallPlan,
    ) -> Result<Admission, WyrdError> {
        let mut conn = self
            .state
            .postgres
            .tenant_conn(call.tenant)
            .await
            .map_err(unavailable)?;
        let admission = GatewayLedger::new(&mut conn)
            .admit(&call.ledger(), plan)
            .await?;
        conn.commit().await.map_err(unavailable)?;
        Ok(admission)
    }

    /// Accounts `execution` in its own committed tenant transaction.
    ///
    /// On failure nothing commits: leases lapse and reconciliation settles the
    /// reservations at their reserved cost once they expire. On success the
    /// appended attempt and call entries are returned for capture and metrics.
    ///
    /// # Errors
    /// Returns `ServiceUnavailable` when storage fails and `Internal` when an
    /// entry violates the ledger contract.
    #[tracing::instrument(name = "gateway.account", skip_all)]
    async fn account(
        state: &AppState,
        call: &AdmittedCall,
        admission: &Admission,
        execution: &CallExecution,
    ) -> Result<Vec<GatewayAccountingEntryV1>, WyrdError> {
        let result = async {
            let mut conn = state
                .postgres
                .tenant_conn(call.tenant)
                .await
                .map_err(unavailable)?;
            let entries = GatewayLedger::new(&mut conn)
                .account(&call.ledger(), admission, execution)
                .await?;
            conn.commit().await.map_err(unavailable)?;
            Ok(entries)
        }
        .await;
        if let Err(error) = &result {
            tracing::error!(call_id = %call.call_id.as_uuid(), error = %error, "gateway call accounting failed");
        }
        result
    }

    /// Terminal facts of `execution` when the admitted policy captures `call`.
    ///
    /// Returns `None` under `Disabled`, so a disabled tenant constructs no
    /// capture state. Only a selected response is kept: the adapter's
    /// credential-free projection taken from `execution`, either JSON or
    /// streamed text kept inline, or media bytes (buffered or generated
    /// speech) as a typed object reference whose bytes are collected for
    /// persistence before publication; or a provider refusal, already
    /// scrubbed. Selected buffered or streamed content without a projection,
    /// because it did not decode, exceeded the capture ceiling, or held the
    /// credential, carries [`CaptureDrop::Payload`]. The terminal time is the completing
    /// attempt's. The outcome and error code start as `execution`'s; a
    /// buffered call whose accounting then fails overrides both with its
    /// settled failure. Request content is resolved later by
    /// [`Self::capture`], after the caller has its answer.
    fn facts(
        call: &AdmittedCall,
        operation: GatewayOperation,
        ingress: IngressDialect,
        stream: bool,
        execution: &mut CallExecution,
        entries: Option<&[GatewayAccountingEntryV1]>,
    ) -> Option<CallFacts> {
        let policy = &call.snapshot.capture;
        if policy.mode == GatewayCaptureMode::Disabled {
            return None;
        }
        let mut objects = PayloadObjects::default();
        let response = if selects(policy, GatewayPayloadField::Response) {
            match (
                execution.capture.take(),
                &execution.response,
                &execution.refusal,
            ) {
                (Some(ResponseCapture::Json(content)), ..) => Ok(Some(content)),
                (Some(ResponseCapture::Media(answer)), ..) => {
                    Ok(Some(objects.reference(&answer.content_type, answer.bytes)))
                }
                (None, None, Some(refusal)) => Ok(Some(refusal.body.clone())),
                (None, Some(ResponseBody::Json(_) | ResponseBody::Media(_)), _) => {
                    Err(CaptureDrop::Payload)
                }
                (None, None, None) if stream && execution.resolved.is_some() => {
                    Err(CaptureDrop::Payload)
                }
                _ => Ok(None),
            }
        } else {
            Ok(None)
        };
        Some(CallFacts {
            call_id: call.call_id,
            tenant: call.tenant,
            caller: call.principal.id,
            request_id: call.request_id.clone(),
            operation,
            ingress,
            streaming: stream,
            requested: call.requested.clone(),
            resolved: execution.resolved.clone(),
            started_at: call.admitted_at,
            terminal_at: execution
                .attempts
                .last()
                .map_or_else(Utc::now, |attempt| attempt.terminal_at),
            outcome: execution.outcome,
            error_code: error_code(execution.outcome),
            attempts: execution.attempts.clone(),
            entries: entries.map(<[_]>::to_vec).unwrap_or_default(),
            policy: policy.clone(),
            request: None,
            response,
            objects,
        })
    }

    /// Projects and enqueues the capture of one terminal call, if selected.
    ///
    /// Runs after the caller has its answer and never waits for publication;
    /// every drop is counted by [`GatewayCapture::record`]. The whole capture —
    /// request content, object get, put, and read-back, and first-use producer
    /// construction — is bounded by the admitted call's absolute deadline, so
    /// a hung dependency cannot hold facts, object bytes, or this task past
    /// the call. On expiry the partial work is dropped, releasing what it
    /// held, no row is enqueued (the enqueue itself never awaits), and one
    /// [`CaptureDrop::Unavailable`] is recorded.
    async fn capture(state: &AppState, call: &AdmittedCall, facts: Option<CallFacts>) {
        let Some(mut facts) = facts else {
            return;
        };
        let work = async {
            if selects(&facts.policy, GatewayPayloadField::Request) {
                match request_content(&call.body, call.media.as_ref(), &mut facts.objects).await {
                    Ok(content) => facts.request = Some(content),
                    Err(_) => {
                        return GatewayCapture::record(call.call_id, Err(CaptureDrop::Payload));
                    }
                }
            }
            match CallCapture::from_facts(facts) {
                Ok(capture) => {
                    state.gateway_capture.publish(state, &capture).await.ok();
                }
                Err(drop) => GatewayCapture::record(call.call_id, Err(drop)),
            }
        }
        .instrument(tracing::info_span!("gateway.capture"));
        if tokio::time::timeout_at(call.deadline, work).await.is_err() {
            GatewayCapture::record(call.call_id, Err(CaptureDrop::Unavailable));
        }
    }

    /// Records the terminal request, attempt, routing, usage, and cost
    /// metrics of one call under closed labels only.
    ///
    /// Request count and duration carry the settled logical-call `outcome`,
    /// which differs from `execution.outcome` when accounting failed after a
    /// completed attempt; attempt and routing metrics keep the provider's
    /// outcome from `execution`.
    ///
    /// Token usage is counted only for the standard `input_tokens` and
    /// `output_tokens` dimensions, and cost only when the ledger priced the
    /// call, so unknown values are never reported as zero.
    fn observe(
        operation: GatewayOperation,
        received: Instant,
        outcome: GatewayCallOutcome,
        execution: &CallExecution,
        entries: Option<&[GatewayAccountingEntryV1]>,
    ) {
        let operation = operation_name(operation);
        let outcome = outcome_name(outcome);
        metrics::counter!("wyrd_gateway_requests_total", "operation" => operation, "outcome" => outcome)
            .increment(1);
        metrics::histogram!("wyrd_gateway_request_duration_seconds", "operation" => operation, "outcome" => outcome)
            .record(received.elapsed().as_secs_f64());
        for attempt in &execution.attempts {
            let outcome = outcome_name(attempt.outcome);
            metrics::counter!("wyrd_gateway_attempts_total", "operation" => operation, "outcome" => outcome)
                .increment(1);
            let seconds = (attempt.terminal_at - attempt.started_at)
                .to_std()
                .map_or(0.0, |elapsed| elapsed.as_secs_f64());
            metrics::histogram!("wyrd_gateway_attempt_duration_seconds", "operation" => operation, "outcome" => outcome)
                .record(seconds);
        }
        let routing = match execution.outcome {
            GatewayCallOutcome::Succeeded if execution.attempts.len() <= 1 => "primary",
            GatewayCallOutcome::Succeeded => "fallback",
            GatewayCallOutcome::Failed => "exhausted",
            GatewayCallOutcome::Cancelled | GatewayCallOutcome::TimedOut => "interrupted",
        };
        metrics::counter!("wyrd_gateway_routing_total", "result" => routing).increment(1);
        let Some(GatewayAccountingEntryV1::CallAccounted {
            normalized_usage,
            cost,
            currency,
            ..
        }) = entries.and_then(<[_]>::last)
        else {
            return;
        };
        for amount in normalized_usage.iter().flatten() {
            let direction = match (amount.dimension.as_str(), amount.unit.as_str()) {
                ("input_tokens", "tokens") => "input",
                ("output_tokens", "tokens") => "output",
                _ => continue,
            };
            if let Ok(tokens) = amount.quantity.as_str().parse::<u64>() {
                metrics::counter!("wyrd_gateway_tokens_total", "operation" => operation, "direction" => direction)
                    .increment(tokens);
            }
        }
        if let (Some(cost), Some(currency)) = (cost, currency)
            && let Ok(value) = cost.as_str().parse::<f64>()
        {
            metrics::histogram!("wyrd_gateway_call_cost", "currency" => currency.as_str().to_owned())
                .record(value);
        }
    }
}

/// The caller's answer and whether any attempt was billable.
type Answer = (Result<GatewayCallResponse, WyrdError>, bool);

/// Execution of one admitted call on a tracked `gateway_tasks` task.
///
/// It owns everything the call needs, so accounting and capture finish even
/// if the caller drops. The shutdown drain waits for it.
struct CallTask {
    /// Server state for the engine, ledger, and capture.
    state: AppState,
    /// Admitted call.
    call: AdmittedCall,
    /// Candidate routes admission reserved.
    plan: CallPlan,
    /// Reservations that accounting settles.
    admission: Admission,
    /// Dialect the caller spoke.
    ingress: IngressDialect,
    /// Whether the caller asked for a stream.
    stream: bool,
    /// Receipt time, for queue delay and request duration.
    received: Instant,
    /// The caller's `gateway.call` span, which receives the settled outcome.
    call_span: tracing::Span,
    /// Call cancellation, fired if the caller drops before the answer.
    cancel: CancellationToken,
}

impl CallTask {
    /// Runs the provider attempts, then settles the call as a buffered answer
    /// or an opened stream, sending the answer on `head`.
    ///
    /// Latency: provider attempts are the dominant wait on the request path.
    /// One active-call gauge unit is held until settlement ends.
    async fn run(self, head: oneshot::Sender<Answer>) {
        let _active = ActiveCall::begin();
        metrics::histogram!(
            "wyrd_gateway_queue_delay_seconds",
            "operation" => operation_name(self.plan.operation)
        )
        .record(self.received.elapsed().as_secs_f64());
        let mut execution = self
            .state
            .gateway_engine
            .execute(CallInput {
                tenant: self.call.tenant,
                snapshot: &self.call.snapshot,
                plan: &self.plan,
                body: &self.call.body,
                ingress: self.ingress,
                media: self.call.media.as_ref(),
                batch: self.call.batch.as_ref(),
                stream: self.stream,
                capture: selects(&self.call.snapshot.capture, GatewayPayloadField::Response),
                deadline: self.call.deadline,
                cancel: &self.cancel,
            })
            .await;
        let end = match &mut execution.response {
            Some(ResponseBody::Events(events) | ResponseBody::MediaStream { events, .. }) => {
                events.end.take()
            }
            _ => None,
        };
        let dispatched = execution.attempts.iter().any(|attempt| attempt.billable);
        match end {
            None => self.settle_buffered(execution, dispatched, head).await,
            Some(end) => self.settle_stream(execution, end, dispatched, head).await,
        }
    }

    /// Accounts a buffered call, then answers the caller, then captures it.
    ///
    /// Latency: the caller waits for the accounting commit, a tenant Postgres
    /// transaction, because an accounting failure must fail the call. The
    /// call then settles as `Failed` with the accounting error code, while
    /// attempt and routing evidence keep the provider's outcome. Capture runs
    /// after the answer is sent and never delays it.
    async fn settle_buffered(
        self,
        mut execution: CallExecution,
        dispatched: bool,
        head: oneshot::Sender<Answer>,
    ) {
        let operation = self.plan.operation;
        let accounted =
            GatewayInvocation::account(&self.state, &self.call, &self.admission, &execution).await;
        let entries = accounted.as_deref().ok();
        let mut facts = GatewayInvocation::facts(
            &self.call,
            operation,
            self.ingress,
            self.stream,
            &mut execution,
            entries,
        );
        let outcome = match (&accounted, facts.as_mut()) {
            (Ok(_), _) => execution.outcome,
            (Err(error), facts) => {
                if let Some(facts) = facts {
                    facts.outcome = GatewayCallOutcome::Failed;
                    facts.error_code = Some(error.code().to_owned());
                }
                GatewayCallOutcome::Failed
            }
        };
        GatewayInvocation::observe(operation, self.received, outcome, &execution, entries);
        record_first_byte(operation, self.received);
        let answer = accounted.and_then(|_| GatewayInvocation::respond(&self.call, execution));
        record_call(&self.call_span, outcome, answer.is_ok());
        // A dropped caller no longer awaits the answer.
        head.send((answer, dispatched)).ok();
        GatewayInvocation::capture(&self.state, &self.call, facts).await;
    }

    /// Answers the caller with the opened stream, waits for the stream to
    /// end, then accounts, observes, and captures the call.
    ///
    /// Latency: the caller gets the stream right away and never waits for
    /// accounting or capture. Output has already reached the caller, so an
    /// accounting failure is only logged. A relay that drops without
    /// reporting its end, as at shutdown, settles the call as `Cancelled`.
    async fn settle_stream(
        self,
        mut execution: CallExecution,
        end: oneshot::Receiver<StreamEnd>,
        dispatched: bool,
        head: oneshot::Sender<Answer>,
    ) {
        let operation = self.plan.operation;
        let resolved = execution.resolved.clone();
        let mut attempts = std::mem::take(&mut execution.attempts);
        let attempt_span = std::mem::replace(&mut execution.attempt_span, tracing::Span::none());
        record_first_byte(operation, self.received);
        head.send((
            GatewayInvocation::respond(&self.call, execution),
            dispatched,
        ))
        .ok();
        let end = end.await.unwrap_or_else(|_| StreamEnd {
            outcome: GatewayCallOutcome::Cancelled,
            terminal_at: Utc::now(),
            usage: AttemptUsage::default(),
            capture: None,
        });
        if let Some(last) = attempts.last_mut() {
            last.outcome = end.outcome;
            last.terminal_at = end.terminal_at;
            last.usage = end.usage;
        }
        attempt_span.record("outcome", outcome_name(end.outcome));
        match end.outcome {
            GatewayCallOutcome::Succeeded => {}
            GatewayCallOutcome::Failed => {
                attempt_span.record("failure_class", FailureClass::AfterOutput.label());
            }
            GatewayCallOutcome::TimedOut | GatewayCallOutcome::Cancelled => {
                if let Some(code) = outcome_error_code(end.outcome) {
                    attempt_span.record("error.code", code);
                }
            }
        }
        record_call(&self.call_span, end.outcome, true);
        let mut accounted = CallExecution {
            outcome: end.outcome,
            response: None,
            resolved,
            attempts,
            refusal: None,
            capture: end.capture,
            attempt_span,
        };
        let entries =
            GatewayInvocation::account(&self.state, &self.call, &self.admission, &accounted).await;
        let entries = entries.as_deref().ok();
        let facts = GatewayInvocation::facts(
            &self.call,
            operation,
            self.ingress,
            self.stream,
            &mut accounted,
            entries,
        );
        GatewayInvocation::observe(operation, self.received, end.outcome, &accounted, entries);
        GatewayInvocation::capture(&self.state, &self.call, facts).await;
    }
}

/// Records a call's closed terminal `outcome` on its `gateway.call` span, and
/// the stable error code of a non-successful call the caller received as an
/// answer: a provider refusal or an opened stream that ended early.
/// [`GatewayInvocation::run`] records the code of a returned error.
fn record_call(span: &tracing::Span, outcome: GatewayCallOutcome, answered: bool) {
    span.record("outcome", outcome_name(outcome));
    if answered && let Some(code) = error_code(outcome) {
        span.record("error.code", code.as_str());
    }
}

/// Records the time from receipt to the answer reaching the caller: the whole
/// buffered answer, or the opening of a stream.
fn record_first_byte(operation: GatewayOperation, received: Instant) {
    metrics::histogram!(
        "wyrd_gateway_time_to_first_byte_seconds",
        "operation" => operation_name(operation)
    )
    .record(received.elapsed().as_secs_f64());
}

/// One unit of the active-calls gauge, held by a call's execution task so
/// every terminal path, including cancellation, releases it.
struct ActiveCall;

impl ActiveCall {
    /// Counts one call as active until the returned guard drops.
    fn begin() -> Self {
        metrics::gauge!("wyrd_gateway_active_calls").increment(1.0);
        Self
    }
}

impl Drop for ActiveCall {
    /// Releases this call's unit of the active-calls gauge.
    fn drop(&mut self) {
        metrics::gauge!("wyrd_gateway_active_calls").decrement(1.0);
    }
}

/// Provider dispatch used until concrete adapters are connected: every
/// attempt fails before reaching a provider, so nothing is billable.
pub(crate) struct NoProviderDispatch;

#[async_trait]
impl ProviderDispatch for NoProviderDispatch {
    /// Refuses the attempt before dispatch.
    async fn dispatch(&self, _attempt: ProviderAttempt<'_>) -> AttemptResult {
        AttemptResult::Failed {
            class: FailureClass::BeforeDispatch,
            usage: AttemptUsage::default(),
        }
    }
}

/// Builds the engine a server uses when none is attached.
pub(crate) fn unconnected_engine() -> GatewayEngine {
    GatewayEngine::new(
        CredentialResolver::default(),
        DeploymentHealth::default(),
        Arc::new(NoProviderDispatch),
    )
}

/// Stable rejection of an invalid call request.
pub(crate) fn invalid_request(error: GatewayContractError) -> WyrdError {
    WyrdError::GatewayInvalidRequest {
        message: error.to_string(),
        details: json!({ "field": error.field, "reason": error.reason }),
    }
}
