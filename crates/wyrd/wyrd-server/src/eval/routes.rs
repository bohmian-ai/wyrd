//! Native `/v1/eval/*` pull-protocol handlers.
//!
//! Mounted inside the authenticated `/v1` group beside
//! `crate::storage::routes::mount`. The `/v1` group has **no structural auth
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

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::routing::post;
use axum::{Json, Router};
use vala_eval::orchestrator::{NextDirective, RunState};
use wyrd_runtime::{Permission, PermissionVerdict, Principal};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::eval::protocol::{
    AgentTurnSubmission, EvalRunOpenRequest, EvalRunOpenResponse, TurnDirective, UserTurnSubmission,
};
use wyrd_spec::vala::ids::{LeaseToken, RunId};
use wyrd_sql::TenantConn;

use crate::auth::AuthenticatedPrincipal;
use crate::error::WyrdErrorResponse;
use crate::state::AppState;

use super::audit::{EvalAuditEvent, EvalAuditKind};
use super::error::{
    eval_engine_error, eval_internal_error, eval_invalid_lease, eval_missing_lease,
    eval_run_not_found, eval_submission_error, eval_too_many_runs, map_card_resolution_error,
};
use super::resolver;
use super::state::{MAX_CONCURRENT_RUNS, RunEntry, sweep_and_count_tenant};

/// Mount the four eval pull-protocol routes into an existing `/v1` router.
///
/// Mirrors `crate::storage::routes::mount`. Every route resolves the principal
/// per-handler; there is no group-level auth layer to rely on.
pub fn mount(router: Router<AppState>, _state: &AppState) -> Router<AppState> {
    router
        .route("/eval/runs", post(open))
        .route("/eval/runs/{run_id}/next", post(next))
        .route("/eval/runs/{run_id}/agent-turn", post(agent_turn))
        .route("/eval/runs/{run_id}/user-turn", post(user_turn))
}

#[tracing::instrument(
    skip(state, principal, req),
    fields(
        wyrd.tenant = %principal.principal.tenant_id,
        wyrd.eval_ref = %req.eval_ref.name.as_str(),
    ),
)]
async fn open(
    State(state): State<AppState>,
    principal: AuthenticatedPrincipal,
    Json(req): Json<EvalRunOpenRequest>,
) -> Result<Json<EvalRunOpenResponse>, WyrdErrorResponse> {
    let principal = principal.principal;
    let tenant = principal.tenant_id;
    let owner = principal.id;

    // Concrete RBAC (spec §2b): gate `evals:run` before any card read. The gate is
    // card-independent, so checking it first denies an unpermissioned principal
    // with 403 regardless of whether the `eval_ref` exists — closing the
    // same-tenant existence oracle — and spares a tenant conn and two DB reads.
    require_eval_run(&state, &principal)?;

    // RLS hops 1 (eval_ref → Eval card) and 2 (Eval.dataset → Data card) run
    // under a single tenant bind. A foreign/missing ref returns 404, fail-closed.
    let mut conn = TenantConn::acquire(&state.pool, tenant)
        .await
        .map_err(|error| {
            WyrdErrorResponse::from(eval_internal_error(format!("tenant conn: {error}")))
        })?;
    let eval_card = resolver::resolve_card(&mut conn, CardKind::Eval, &req.eval_ref)
        .await
        .map_err(|error| WyrdErrorResponse::from(map_card_resolution_error(&error)))?;

    let dataset = resolver::dataset_ref(&eval_card).map_err(WyrdErrorResponse::from)?;
    let data_card = resolver::resolve_card(&mut conn, CardKind::Data, dataset.as_card_ref())
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

    state.eval_audit.record(&EvalAuditEvent {
        kind: EvalAuditKind::RunOpen,
        principal: owner,
        tenant,
        eval_ref: req.eval_ref,
        run_id: run_id.clone(),
    });

    Ok(Json(EvalRunOpenResponse {
        run_id,
        lease_token: lease,
    }))
}

#[tracing::instrument(skip(state, principal, headers), fields(wyrd.run_id = %run_id))]
async fn next(
    State(state): State<AppState>,
    principal: AuthenticatedPrincipal,
    Path(run_id): Path<RunId>,
    headers: HeaderMap,
) -> Result<Json<TurnDirective>, WyrdErrorResponse> {
    let principal = principal.principal;
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
            let eval_ref = {
                let mut run = entry.state.lock().await;
                let eval_ref = run.eval_ref.clone();
                run.ack_run_complete();
                eval_ref
            };
            {
                let mut runs = state.eval_runs.lock().map_err(|_| {
                    WyrdErrorResponse::from(eval_internal_error("eval run map lock poisoned"))
                })?;
                runs.remove(&(tenant, run_id.clone()));
            }
            state.eval_audit.record(&EvalAuditEvent {
                kind: EvalAuditKind::RunComplete,
                principal: principal.id,
                tenant,
                eval_ref,
                run_id: run_id.clone(),
            });
            Ok(Json(directive))
        }
        TurnDirective::AgentTurn { .. } => Ok(Json(directive)),
    }
}

#[tracing::instrument(skip(state, principal, headers, sub), fields(wyrd.run_id = %run_id))]
async fn agent_turn(
    State(state): State<AppState>,
    principal: AuthenticatedPrincipal,
    Path(run_id): Path<RunId>,
    headers: HeaderMap,
    Json(sub): Json<AgentTurnSubmission>,
) -> Result<StatusCode, WyrdErrorResponse> {
    let principal = principal.principal;
    let entry = lookup(&state, principal.tenant_id, &run_id, &principal, &headers)?;
    entry
        .state
        .lock()
        .await
        .submit_agent_turn(sub)
        .map_err(|error| WyrdErrorResponse::from(eval_submission_error(&error)))?;
    Ok(StatusCode::ACCEPTED)
}

#[tracing::instrument(skip(state, principal, headers, sub), fields(wyrd.run_id = %run_id))]
async fn user_turn(
    State(state): State<AppState>,
    principal: AuthenticatedPrincipal,
    Path(run_id): Path<RunId>,
    headers: HeaderMap,
    Json(sub): Json<UserTurnSubmission>,
) -> Result<StatusCode, WyrdErrorResponse> {
    let principal = principal.principal;
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

/// Constant-time per-run lease check, retained beneath principal identity.
fn check_lease(headers: &HeaderMap, entry: &RunEntry) -> Result<(), WyrdErrorResponse> {
    use subtle::ConstantTimeEq;

    let value = headers
        .get(header::AUTHORIZATION)
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

/// `Permission::eval_run` gate against the tenant-resolved principal.
///
/// Uses the shared synchronous RBAC checker (`AppState::permission_check`), a pure
/// function over the principal's `effective_permissions`. A `Deny` maps to a 403
/// `WyrdError`, mirroring the `check_authz` handler's `missing_permission` arm.
fn require_eval_run(state: &AppState, principal: &Principal) -> Result<(), WyrdErrorResponse> {
    let required = Permission::eval_run();
    match state.permission_check.check(principal, &required) {
        PermissionVerdict::Allow => Ok(()),
        PermissionVerdict::Deny { .. } => {
            tracing::warn!(
                wyrd.required = %required,
                wyrd.principal = %principal.id,
                "eval run creation denied: principal lacks evals:run",
            );
            Err(WyrdErrorResponse::from(WyrdError::PermissionDeniedRbac {
                message: "evals:run permission required to open an eval run".to_owned(),
                details: serde_json::json!({ "required": required.to_string() }),
            }))
        }
    }
}

/// Mint a fresh per-run lease token.
fn mint_lease() -> Result<LeaseToken, WyrdError> {
    LeaseToken::new(format!("lease-{}", RunId::new().as_str()))
        .map_err(|error| eval_internal_error(format!("lease mint failed: {error}")))
}
