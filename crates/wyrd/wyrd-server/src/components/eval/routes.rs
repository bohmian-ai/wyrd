//! Native `/v1/eval/*` pull-protocol handlers.
//!
//! Mounted inside the authenticated `/v1` group beside
//! `crate::components::storage::storage_router`. The `/v1` group has **no structural auth
//! layer** — every handler here takes the [`AuthenticatedPrincipal`] extractor,
//! so a route omitting it would be silently unauthenticated.
//!
//! Tenant isolation is enforced on every path: `open` resolves `eval_ref` and
//! its `dataset` under one `TenantConn` (RLS), stamps the run's owner, and keys
//! the run map by `(tenant_id, run_id)`; `next`/turn handlers look up by the
//! composite key **first** (miss → 404) so a foreign `run_id` is
//! indistinguishable from a nonexistent one.

use std::sync::Arc;
use std::time::Instant;

use crate::audit;
use crate::components::auth::{AuthenticatedPrincipal, Caller};
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use vala_eval::orchestrator::{NextDirective, RunState};
use wyrd_runtime::{Permission, Principal};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::protocol::{
    AgentTurnSubmission, EvalRunOpenRequest, EvalRunOpenResponse, TurnDirective, UserTurnSubmission,
};
use wyrd_spec::vala::ids::{LeaseToken, RunId};

use super::error::{
    eval_engine_error, eval_internal_error, eval_invalid_lease, eval_missing_lease,
    eval_run_not_found, eval_submission_error, eval_too_many_runs, map_card_resolution_error,
};
use super::resolver;
use super::state::{MAX_CONCURRENT_RUNS, RunEntry, sweep_and_count_tenant};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

/// Build the four eval pull-protocol routes for the `/v1` group.
///
/// Mirrors `crate::components::storage::storage_router`. Every route resolves the principal
/// per-handler; there is no group-level auth layer to rely on.
pub fn eval_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(open))
        .routes(routes!(next))
        .routes(routes!(agent_turn))
        .routes(routes!(user_turn))
}

/// Open one evaluation run and lease it to this caller.
#[utoipa::path(
    post,
    path = "/eval/runs",
    request_body = EvalRunOpenRequest,
    responses(
        (status = 200, description = "Run opened; the lease token is returned once",
         body = EvalRunOpenResponse),
        (status = 401, description = "Authentication required (WYRD_AUTH_401_UNAUTHENTICATED)",
         body = WyrdProblem),
        (status = 403, description = "The `evals:run` permission is required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "The eval or its dataset card does not exist in this \
          tenant (WYRD_EVAL_404_RUN_NOT_FOUND)", body = WyrdProblem),
        (status = 429, description = "This tenant already holds the maximum number of open \
          runs (WYRD_EVAL_429_TOO_MANY_RUNS)", body = WyrdProblem),
        (status = 500, description = "The run could not be opened, or the authorization \
          decision could not be audited (WYRD_EVAL_500_RUN_FAILED, \
          WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The revocation store could not vouch for the token (\
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Eval"
)]
#[tracing::instrument(
    skip(state, caller, req),
    fields(
        wyrd.tenant = %caller.data_tenant_id,
        wyrd.eval_ref = %req.eval_ref.name.as_str(),
    ),
)]
async fn open(
    State(state): State<AppState>,
    caller: Caller,
    Json(req): Json<EvalRunOpenRequest>,
) -> Result<Json<EvalRunOpenResponse>, WyrdErrorResponse> {
    let tenant = caller.data_tenant_id;
    let owner = caller.principal.id;

    // Concrete RBAC (spec §2b): gate `evals:run` before any card read. The gate is
    // card-independent, so checking it first denies an unpermissioned principal
    // with 403 regardless of whether the `eval_ref` exists — closing the
    // same-tenant existence oracle — and spares a tenant conn and two DB reads.
    require_eval_run(&state, &caller, &req.eval_ref).await?;

    // RLS hops 1 (eval_ref → Eval card) and 2 (Eval.dataset → Data card) run
    // under a single tenant bind. A foreign/missing ref returns 404, fail-closed.
    let mut conn = state.postgres.tenant_conn(tenant).await.map_err(|error| {
        WyrdErrorResponse::from(eval_internal_error(format!("tenant conn: {error}")))
    })?;
    let eval_card = resolver::resolve_card(&mut conn, CardKind::Eval, &req.eval_ref)
        .await
        .map_err(|error| WyrdErrorResponse::from(map_card_resolution_error(&error)))?;

    let dataset = resolver::dataset_ref(&eval_card).map_err(WyrdErrorResponse::from)?;
    let dataset_ref = dataset.as_card_ref().ok_or_else(|| {
        WyrdErrorResponse::from(eval_internal_error("eval dataset is not a card reference"))
    })?;
    let data_card = resolver::resolve_card(&mut conn, CardKind::Data, dataset_ref)
        .await
        .map_err(|error| WyrdErrorResponse::from(map_card_resolution_error(&error)))?;
    conn.commit().await.map_err(|error| {
        WyrdErrorResponse::from(eval_internal_error(format!("commit: {error}")))
    })?;

    // Hop 3: scenario bytes derived solely from the tenant-verified Data card path.
    let scenarios = resolver::load_scenarios(&state.storage, tenant, &data_card)
        .await
        .map_err(WyrdErrorResponse::from)?;

    let run_state = RunState::open(req.eval_ref.clone(), req.simulated_user, scenarios);
    let run_id = run_state.run_id.clone();
    let lease = mint_lease().map_err(WyrdErrorResponse::from)?;

    // Per-tenant TTL sweep, cap, and owner-stamped insert under one map lock. The
    // count is derived by filtering the composite-keyed map after eviction — no
    // side counter that could drift against lazy eviction.
    {
        let mut runs = state.eval_runs.lock().map_err(|_| {
            WyrdErrorResponse::from(eval_internal_error("eval run map lock poisoned"))
        })?;
        let tenant_open = sweep_and_count_tenant(&mut runs, tenant);
        if tenant_open >= MAX_CONCURRENT_RUNS {
            return Err(WyrdErrorResponse::from(eval_too_many_runs()));
        }
        runs.insert(
            (tenant, run_id.clone()),
            Arc::new(RunEntry {
                state: Arc::new(tokio::sync::Mutex::new(run_state)),
                lease: lease.clone(),
                owner,
                opened_at: Instant::now(),
            }),
        );
    }

    Ok(Json(EvalRunOpenResponse {
        run_id,
        lease_token: lease,
    }))
}

/// Advance one leased run by a single protocol step.
#[utoipa::path(
    post,
    path = "/eval/runs/{run_id}/next",
    params(("run_id" = String, Path, description = "Run to advance")),
    responses(
        (status = 200, description = "The next directive for this run", body = TurnDirective),
        (status = 401, description = "Authentication is required and the lease token must be \
          presented (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_EVAL_401_MISSING_LEASE)",
         body = WyrdProblem),
        (status = 403, description = "The presented lease does not hold this run \
          (WYRD_EVAL_403_INVALID_LEASE)", body = WyrdProblem),
        (status = 404, description = "No such run for this tenant and principal \
          (WYRD_EVAL_404_RUN_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "The run could not be advanced \
          (WYRD_EVAL_500_RUN_FAILED)", body = WyrdProblem),
        (status = 503, description = "The revocation store could not vouch for the token \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Eval"
)]
#[tracing::instrument(skip(state, principal, headers), fields(wyrd.run_id = %run_id))]
async fn next(
    State(state): State<AppState>,
    principal: AuthenticatedPrincipal,
    Path(run_id): Path<RunId>,
    headers: HeaderMap,
) -> Result<Json<TurnDirective>, WyrdErrorResponse> {
    let principal = Principal::from(principal);
    let tenant = principal.tenant_id;
    let entry = lookup(&state, tenant, &run_id, &principal, &headers)?;

    // Advance the engine one step. Unlike the removed server-simulator loop, no
    // directive is consumed server-side here: each directive is returned to the
    // client, so this is a single step, not a loop.
    let directive = {
        let mut run = entry.state.lock().await;
        let NextDirective(directive) = run
            .next()
            .map_err(|error| WyrdErrorResponse::from(eval_engine_error(&error)))?;
        directive
    };

    match directive {
        TurnDirective::UserTurnNeeded { .. } => {
            if entry.state.lock().await.wants_server_simulated_turn() {
                return Err(WyrdErrorResponse::from(eval_internal_error(
                    "server-simulated user mode is not supported on this surface",
                )));
            }
            Ok(Json(directive))
        }
        TurnDirective::ScenarioComplete { .. } => {
            entry.state.lock().await.ack_scenario_complete();
            Ok(Json(directive))
        }
        TurnDirective::RunComplete => {
            entry.state.lock().await.ack_run_complete();
            {
                let mut runs = state.eval_runs.lock().map_err(|_| {
                    WyrdErrorResponse::from(eval_internal_error("eval run map lock poisoned"))
                })?;
                runs.remove(&(tenant, run_id.clone()));
            }
            Ok(Json(directive))
        }
        TurnDirective::AgentTurn { .. } => Ok(Json(directive)),
    }
}

/// Submit the agent's half of one turn.
#[utoipa::path(
    post,
    path = "/eval/runs/{run_id}/agent-turn",
    params(("run_id" = String, Path, description = "Run the turn belongs to")),
    request_body = AgentTurnSubmission,
    responses(
        (status = 202, description = "The turn was accepted"),
        (status = 401, description = "Authentication is required and the lease token must be \
          presented (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_EVAL_401_MISSING_LEASE)",
         body = WyrdProblem),
        (status = 403, description = "The presented lease does not hold this run \
          (WYRD_EVAL_403_INVALID_LEASE)", body = WyrdProblem),
        (status = 404, description = "No such run for this tenant and principal \
          (WYRD_EVAL_404_RUN_NOT_FOUND)", body = WyrdProblem),
        (status = 409, description = "The submission does not match the turn the run is \
          waiting on (WYRD_EVAL_409_SUBMISSION_MISMATCH)", body = WyrdProblem),
        (status = 503, description = "The revocation store could not vouch for the token \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Eval"
)]
#[tracing::instrument(skip(state, principal, headers, sub), fields(wyrd.run_id = %run_id))]
async fn agent_turn(
    State(state): State<AppState>,
    principal: AuthenticatedPrincipal,
    Path(run_id): Path<RunId>,
    headers: HeaderMap,
    Json(sub): Json<AgentTurnSubmission>,
) -> Result<StatusCode, WyrdErrorResponse> {
    let principal = Principal::from(principal);
    let entry = lookup(&state, principal.tenant_id, &run_id, &principal, &headers)?;
    entry
        .state
        .lock()
        .await
        .submit_agent_turn(sub)
        .map_err(|error| WyrdErrorResponse::from(eval_submission_error(&error)))?;
    Ok(StatusCode::ACCEPTED)
}

/// Submit the simulated user's half of one turn.
#[utoipa::path(
    post,
    path = "/eval/runs/{run_id}/user-turn",
    params(("run_id" = String, Path, description = "Run the turn belongs to")),
    request_body = UserTurnSubmission,
    responses(
        (status = 202, description = "The turn was accepted"),
        (status = 401, description = "Authentication is required and the lease token must be \
          presented (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_EVAL_401_MISSING_LEASE)",
         body = WyrdProblem),
        (status = 403, description = "The presented lease does not hold this run \
          (WYRD_EVAL_403_INVALID_LEASE)", body = WyrdProblem),
        (status = 404, description = "No such run for this tenant and principal \
          (WYRD_EVAL_404_RUN_NOT_FOUND)", body = WyrdProblem),
        (status = 409, description = "The submission does not match the turn the run is \
          waiting on (WYRD_EVAL_409_SUBMISSION_MISMATCH)", body = WyrdProblem),
        (status = 503, description = "The revocation store could not vouch for the token \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Eval"
)]
#[tracing::instrument(skip(state, principal, headers, sub), fields(wyrd.run_id = %run_id))]
async fn user_turn(
    State(state): State<AppState>,
    principal: AuthenticatedPrincipal,
    Path(run_id): Path<RunId>,
    headers: HeaderMap,
    Json(sub): Json<UserTurnSubmission>,
) -> Result<StatusCode, WyrdErrorResponse> {
    let principal = Principal::from(principal);
    let entry = lookup(&state, principal.tenant_id, &run_id, &principal, &headers)?;
    entry
        .state
        .lock()
        .await
        .submit_user_turn(sub)
        .map_err(|error| WyrdErrorResponse::from(eval_submission_error(&error)))?;
    Ok(StatusCode::ACCEPTED)
}

/// Composite-key lookup first (miss → 404), then owner + lease enforcement.
///
/// Keying the lookup itself — not a post-hoc tenant compare — closes the
/// existence oracle: a foreign `run_id` is a 404, identical to a nonexistent one.
/// An owner mismatch is likewise a 404 (never a 403) so run existence does not
/// leak across principals within a tenant.
fn lookup(
    state: &AppState,
    tenant: wyrd_spec::DataTenantId,
    run_id: &RunId,
    principal: &Principal,
    headers: &HeaderMap,
) -> Result<Arc<RunEntry>, WyrdErrorResponse> {
    let entry = {
        let runs = state.eval_runs.lock().map_err(|_| {
            WyrdErrorResponse::from(eval_internal_error("eval run map lock poisoned"))
        })?;
        runs.get(&(tenant, run_id.clone())).map(Arc::clone)
    };
    let entry = entry.ok_or_else(|| WyrdErrorResponse::from(eval_run_not_found()))?;
    if entry.owner != principal.id {
        return Err(WyrdErrorResponse::from(eval_run_not_found()));
    }
    check_lease(headers, &entry)?;
    Ok(entry)
}

/// Header carrying a run's lease token.
///
/// The lease is a second credential on a request that already carries the
/// caller's Wyrd token, so it needs a header of its own. It is not
/// `Authorization`: that header belongs to the calling application and no Wyrd
/// surface reads it.
pub(crate) const EVAL_LEASE_HEADER: &str = "x-wyrd-eval-lease";

/// Constant-time per-run lease check, retained beneath principal identity.
///
/// Every protocol call on an open run passes through here after the caller's own
/// token has authenticated them: the lease is what binds a permitted caller to
/// the one run it opened, so a principal that may run evals still cannot drive
/// another's. The comparison is constant-time because the lease is a secret and
/// the caller controls the candidate.
///
/// # Errors
/// Returns `WYRD_EVAL_401_MISSING_LEASE` when [`EVAL_LEASE_HEADER`] is absent,
/// not valid UTF-8, carries no space-separated scheme, names a scheme other than
/// `Bearer`, or carries an empty token — a caller that presented nothing usable
/// is told the same thing however it failed to. Returns
/// `WYRD_EVAL_403_INVALID_LEASE` when a well-formed lease does not match the one
/// minted for this run.
fn check_lease(headers: &HeaderMap, entry: &RunEntry) -> Result<(), WyrdErrorResponse> {
    use subtle::ConstantTimeEq;

    let value = headers
        .get(EVAL_LEASE_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| WyrdErrorResponse::from(eval_missing_lease()))?;
    let (scheme, token) = value
        .split_once(' ')
        .ok_or_else(|| WyrdErrorResponse::from(eval_missing_lease()))?;
    if !scheme.eq_ignore_ascii_case("Bearer") || token.is_empty() {
        return Err(WyrdErrorResponse::from(eval_missing_lease()));
    }
    if entry
        .lease
        .as_str()
        .as_bytes()
        .ct_eq(token.as_bytes())
        .into()
    {
        Ok(())
    } else {
        Err(WyrdErrorResponse::from(eval_invalid_lease()))
    }
}

/// Evaluate and audit `Permission::eval_run` before an eval run is opened.
///
/// Opening a run is the receiving authorization boundary for the eval surface,
/// so the verdict — allowed or denied — is recorded as one canonical audit event
/// naming the requested eval before the run is created or refused. The append is
/// fail-closed: a run is never opened on an unrecorded decision.
///
/// A denial keeps the eval surface's own refusal body, naming the required
/// permission as its wire string, so auditing the verdict changes no response.
///
/// # Errors
/// Returns `WYRD_PERMISSION_403_DENIED_RBAC` when the principal lacks
/// `evals:run`, and the audit-unavailable error when either outcome cannot be
/// recorded.
async fn require_eval_run(
    state: &AppState,
    caller: &Caller,
    eval_ref: &CardRef,
) -> Result<(), WyrdErrorResponse> {
    audit::authorize(
        state,
        caller,
        &Permission::eval_run(),
        "eval.run.open",
        &format!("eval:{}", eval_ref.name.as_str()),
    )
    .await
    .map_err(|error| match error {
        WyrdError::PermissionDeniedRbac { .. } => WyrdError::PermissionDeniedRbac {
            message: "evals:run permission required to open an eval run".to_owned(),
            details: serde_json::json!({ "required": Permission::eval_run().to_string() }),
        },
        other => other,
    })
    .map_err(WyrdErrorResponse::from)
}

/// Mint a fresh per-run lease token.
fn mint_lease() -> Result<LeaseToken, WyrdError> {
    LeaseToken::new(format!("lease-{}", RunId::new().as_str()))
        .map_err(|error| eval_internal_error(format!("lease mint failed: {error}")))
}
