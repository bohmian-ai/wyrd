use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, Request, StatusCode, header},
    middleware::{Next, from_fn_with_state},
    response::IntoResponse,
    routing::post,
};
use chrono::Utc;
use dashmap::DashMap;
use thiserror::Error;
use tokio::sync::Mutex;
use vala_eval::orchestrator::{
    NextDirective, OrchestratorError, RunState, ScenarioScoring, ServerSimulatedUser, SharedRun,
};
use vala_eval::{EvalResults, RunIdentity, ScenarioAggregationInput, ScenarioExecutionResults};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::{
    EvalScenario,
    protocol::{
        AgentTurnSubmission, EvalRunOpenRequest, EvalRunOpenResponse, TurnDirective,
        UserTurnSubmission,
    },
};
use wyrd_spec::vala::ids::{LeaseToken, RunId};

/// Maximum number of concurrently open eval runs. New opens are rejected with
/// 429 when this cap is reached. Sized for a single-server dev deployment;
/// production systems with durable state storage can raise this.
const MAX_CONCURRENT_RUNS: usize = 100;

/// Runs older than this are eligible for lazy eviction on the next `open`
/// call. A background sweep would be cleaner but requires async context at
/// construction time; lazy eviction is safe and keeps `AppState::new` sync.
const RUN_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Environment variable name for the preshared API key.
///
/// When set, all eval routes require `Authorization: Bearer <key>`.
/// This is a stopgap until the Wyrd JWT scope-verification auth plane is wired.
const API_KEY_ENV: &str = "WYRD_API_KEY";

type ScenarioLoader =
    dyn Fn(&CardRef) -> Result<Vec<EvalScenario>, OrchestratorError> + Send + Sync;
type LeaseIssuer = dyn Fn() -> Result<LeaseToken, HttpError> + Send + Sync;

/// Public HTTP-surface errors for eval pull-protocol routes.
#[derive(Debug, Error, wyrd_error_derive::WyrdError)]
pub enum HttpError {
    /// Run id not found in the in-memory map.
    #[error("eval run {id} not found")]
    #[wyrd_error(
        code = "WYRD_EVAL_404_RUN_NOT_FOUND",
        status = 404,
        title = "Eval run not found",
        remediation = "Re-open a run with POST /api/v1/eval/runs."
    )]
    RunNotFound {
        /// Run id from the route path.
        id: RunId,
    },

    /// Request lacked the bearer lease header.
    #[error("missing lease token on protected eval-run route")]
    #[wyrd_error(
        code = "WYRD_EVAL_401_MISSING_LEASE",
        status = 401,
        title = "Missing lease token",
        remediation = "Send the lease_token from EvalRunOpenResponse in the Authorization: Bearer <token> header."
    )]
    MissingLease,

    /// Request carried a lease that does not match the run.
    #[error("lease token does not match the lease issued for run {id}")]
    #[wyrd_error(
        code = "WYRD_EVAL_403_INVALID_LEASE",
        status = 403,
        title = "Invalid lease token",
        remediation = "Re-open the run via POST /api/v1/eval/runs; leases are bound to one run."
    )]
    InvalidLease {
        /// Run id whose lease was challenged.
        id: RunId,
    },

    /// Submission did not match the outstanding directive.
    #[error("submission rejected: {source}")]
    #[wyrd_error(
        code = "WYRD_EVAL_409_SUBMISSION_MISMATCH",
        status = 409,
        title = "Eval submission did not match outstanding directive",
        remediation = "Call POST /api/v1/eval/runs/{run_id}/next to retrieve the outstanding directive and retry."
    )]
    Submission {
        /// Underlying state-machine error.
        #[source]
        source: OrchestratorError,
    },

    /// Engine, simulator, scenario loading, or server configuration failure.
    #[error("eval run failed: {source}")]
    #[wyrd_error(
        code = "WYRD_EVAL_500_RUN_FAILED",
        status = 500,
        title = "Eval run failed",
        remediation = "Inspect the eval route logs and retry after correcting the underlying server or provider issue."
    )]
    Engine {
        /// Underlying orchestrator error.
        #[source]
        source: OrchestratorError,
    },

    /// Too many concurrent eval runs; client must retry after existing runs complete.
    #[error("too many concurrent eval runs; retry after an existing run completes")]
    #[wyrd_error(
        code = "WYRD_EVAL_429_TOO_MANY_RUNS",
        status = 429,
        title = "Too many concurrent eval runs",
        remediation = "Wait for an existing run to complete, then retry."
    )]
    TooManyRuns,

    /// API key missing or invalid on a protected eval route.
    #[error("eval route requires a valid WYRD_API_KEY bearer token")]
    #[wyrd_error(
        code = "WYRD_EVAL_401_API_KEY_INVALID",
        status = 401,
        title = "Invalid or missing API key",
        remediation = "Set WYRD_API_KEY on the server and pass it as Authorization: Bearer <key>."
    )]
    ApiKeyInvalid,
}

impl HttpError {
    fn problem_json(&self) -> serde_json::Value {
        let detail = if self.status() >= 500 {
            tracing::error!(
                wyrd.error.code = self.code(),
                wyrd.error.detail = %self,
                "eval route internal error"
            );
            "Internal server error; see server logs".to_owned()
        } else {
            self.to_string()
        };
        serde_json::json!({
            "type": format!("https://wyrd.dev/problems/{}", self.code()),
            "title": self.title(),
            "status": self.status(),
            "detail": detail,
            "code": self.code(),
            "remediation": self.remediation(),
        })
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> axum::response::Response {
        let status = match StatusCode::from_u16(self.status()) {
            Ok(status) => status,
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(self.problem_json())).into_response()
    }
}

/// Per-run shared state.
pub struct RunEntry {
    /// The state machine.
    pub state: SharedRun,
    /// Per-scenario engine results.
    pub scenario_results: Mutex<Vec<ScenarioExecutionResults>>,
    /// Per-scenario aggregation inputs.
    pub scenario_aggregations: Mutex<Vec<ScenarioAggregationInput>>,
    /// Final run-level result after `RunComplete`.
    pub run_result: Mutex<Option<EvalResults>>,
    /// Bearer lease minted at open.
    pub lease: LeaseToken,
    /// Wall-clock instant when this run was opened, used for TTL eviction.
    pub opened_at: Instant,
}

/// Router-level eval state.
#[derive(Clone)]
pub struct AppState {
    /// Open runs keyed by the core `RunId`.
    pub runs: Arc<DashMap<RunId, Arc<RunEntry>>>,
    /// Optional scoring engine. Tests and configured servers inject it.
    pub scoring: Option<Arc<ScenarioScoring>>,
    /// Optional server-side simulator.
    pub simulator: Option<Arc<ServerSimulatedUser>>,
    /// Scenario loader for a requested Eval card.
    pub scenarios_for_eval: Arc<ScenarioLoader>,
    /// Fresh per-run lease issuer.
    pub lease_issuer: Arc<LeaseIssuer>,
    /// Optional preshared API key loaded from `WYRD_API_KEY` at construction.
    /// When `Some`, every eval route requires `Authorization: Bearer <key>`.
    pub api_key: Option<Arc<str>>,
}

impl AppState {
    /// Build eval route state from injected services.
    ///
    /// Reads `WYRD_API_KEY` from the environment at construction time. When the
    /// variable is set, every eval route requires a matching bearer token.
    #[must_use]
    pub fn new(scenarios_for_eval: Arc<ScenarioLoader>, lease_issuer: Arc<LeaseIssuer>) -> Self {
        let api_key = std::env::var(API_KEY_ENV)
            .ok()
            .map(|k| Arc::from(k.as_str()));
        Self {
            runs: Arc::new(DashMap::new()),
            scoring: None,
            simulator: None,
            scenarios_for_eval,
            lease_issuer,
            api_key,
        }
    }

    /// Build route state for the unified server before registry-backed eval
    /// dependencies exist.
    #[must_use]
    pub fn unconfigured() -> Self {
        Self::new(
            Arc::new(|_| {
                Err(OrchestratorError::EmbeddedCallback {
                    reason: "eval HTTP routes are mounted but no scenario loader is configured"
                        .to_owned(),
                })
            }),
            Arc::new(default_lease_token),
        )
    }

    /// Override the API key. Useful in tests that need auth enabled without
    /// reading the environment.
    #[must_use]
    pub fn with_api_key(mut self, key: impl Into<Arc<str>>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Attach a scoring engine.
    #[must_use]
    pub fn with_scoring(mut self, scoring: Arc<ScenarioScoring>) -> Self {
        self.scoring = Some(scoring);
        self
    }

    /// Attach a server-side simulator.
    #[must_use]
    pub fn with_simulator(mut self, simulator: Arc<ServerSimulatedUser>) -> Self {
        self.simulator = Some(simulator);
        self
    }
}

fn default_lease_token() -> Result<LeaseToken, HttpError> {
    LeaseToken::new(format!("lease-{}", RunId::new().as_str())).map_err(|source| {
        HttpError::Engine {
            source: OrchestratorError::Invariant {
                reason: format!("generated lease token failed validation: {source}"),
            },
        }
    })
}

/// Build the eval pull-protocol router. `wyrd-server` mounts it at
/// `/api/v1/eval`.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/runs", post(open))
        .route("/runs/{run_id}/next", post(next))
        .route("/runs/{run_id}/agent-turn", post(agent_turn))
        .route("/runs/{run_id}/user-turn", post(user_turn))
        .layer(from_fn_with_state(state.clone(), api_key_auth))
        .with_state(state)
}

/// Preshared-key auth middleware.
///
/// When `WYRD_API_KEY` is set, all eval routes require
/// `Authorization: Bearer <key>`. Uses constant-time comparison to resist
/// timing side channels.
///
/// This is a stopgap until Wyrd JWT scope-verification is wired in.
async fn api_key_auth(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> axum::response::Response {
    let Some(expected) = state.api_key.as_ref() else {
        return next.run(req).await;
    };

    use subtle::ConstantTimeEq;
    let valid = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|token| token.as_bytes().ct_eq(expected.as_bytes()).into())
        .unwrap_or(false);

    if valid {
        next.run(req).await
    } else {
        HttpError::ApiKeyInvalid.into_response()
    }
}

#[tracing::instrument(skip(state, req), fields(wyrd.eval_ref = %req.eval_ref.name.as_str()))]
async fn open(
    State(state): State<AppState>,
    Json(req): Json<EvalRunOpenRequest>,
) -> Result<Json<EvalRunOpenResponse>, HttpError> {
    // Lazy TTL eviction: sweep expired entries before checking the cap so a
    // burst of stale runs does not starve new callers.
    state
        .runs
        .retain(|_, entry| entry.opened_at.elapsed() < RUN_TTL);

    if state.runs.len() >= MAX_CONCURRENT_RUNS {
        return Err(HttpError::TooManyRuns);
    }

    let scenarios =
        (state.scenarios_for_eval)(&req.eval_ref).map_err(|source| HttpError::Engine { source })?;
    let run_state = RunState::open(req.eval_ref, req.simulated_user, scenarios);
    let run_id = run_state.run_id.clone();
    let lease_token = (state.lease_issuer)()?;
    let entry = Arc::new(RunEntry {
        state: Arc::new(Mutex::new(run_state)),
        scenario_results: Mutex::new(Vec::new()),
        scenario_aggregations: Mutex::new(Vec::new()),
        run_result: Mutex::new(None),
        lease: lease_token.clone(),
        opened_at: Instant::now(),
    });
    state.runs.insert(run_id.clone(), entry);
    Ok(Json(EvalRunOpenResponse {
        run_id,
        lease_token,
    }))
}

#[tracing::instrument(skip(state, headers), fields(wyrd.run_id = %run_id))]
async fn next(
    State(state): State<AppState>,
    Path(run_id): Path<RunId>,
    headers: HeaderMap,
) -> Result<Json<TurnDirective>, HttpError> {
    let entry = run_entry(&state, &run_id)?;
    check_lease(&headers, &entry, &run_id)?;

    loop {
        let directive = {
            let mut run = entry.state.lock().await;
            let NextDirective(directive) =
                run.next().map_err(|source| HttpError::Engine { source })?;
            directive
        };

        match directive {
            TurnDirective::UserTurnNeeded {
                scenario_id,
                turn,
                history,
            } => {
                let wants_server = entry.state.lock().await.wants_server_simulated_turn();
                if !wants_server {
                    return Ok(Json(TurnDirective::UserTurnNeeded {
                        scenario_id,
                        turn,
                        history,
                    }));
                }
                let simulator = state.simulator.as_ref().ok_or_else(|| HttpError::Engine {
                    source: OrchestratorError::EmbeddedCallback {
                        reason: "SimulatedUserMode::Server requires a server simulator".to_owned(),
                    },
                })?;
                let scenario = entry
                    .state
                    .lock()
                    .await
                    .current_scenario()
                    .cloned()
                    .ok_or_else(|| HttpError::Engine {
                        source: OrchestratorError::Invariant {
                            reason: "server simulation requires an active scenario".to_owned(),
                        },
                    })?;
                let simulated = simulator
                    .next_turn(&scenario, &history)
                    .await
                    .map_err(|source| HttpError::Engine { source })?;
                entry
                    .state
                    .lock()
                    .await
                    .submit_simulated_user(
                        scenario_id,
                        turn,
                        simulated.message,
                        simulated.goal_achieved,
                    )
                    .map_err(|source| HttpError::Submission { source })?;
            }
            TurnDirective::ScenarioComplete { scenario_id } => {
                score_completed_scenario(&state, &entry, &scenario_id).await?;
                entry.state.lock().await.ack_scenario_complete();
                return Ok(Json(TurnDirective::ScenarioComplete { scenario_id }));
            }
            TurnDirective::RunComplete => {
                finalize_run(&state, &entry).await?;
                entry.state.lock().await.ack_run_complete();
                return Ok(Json(TurnDirective::RunComplete));
            }
            TurnDirective::AgentTurn {
                scenario_id,
                turn,
                message,
                history,
            } => {
                tracing::Span::current().record("wyrd.eval.scenario_id", scenario_id.as_str());
                return Ok(Json(TurnDirective::AgentTurn {
                    scenario_id,
                    turn,
                    message,
                    history,
                }));
            }
        }
    }
}

#[tracing::instrument(skip(state, headers, sub), fields(wyrd.run_id = %run_id))]
async fn agent_turn(
    State(state): State<AppState>,
    Path(run_id): Path<RunId>,
    headers: HeaderMap,
    Json(sub): Json<AgentTurnSubmission>,
) -> Result<StatusCode, HttpError> {
    let entry = run_entry(&state, &run_id)?;
    check_lease(&headers, &entry, &run_id)?;
    entry
        .state
        .lock()
        .await
        .submit_agent_turn(sub)
        .map_err(|source| HttpError::Submission { source })?;
    Ok(StatusCode::ACCEPTED)
}

#[tracing::instrument(skip(state, headers, sub), fields(wyrd.run_id = %run_id))]
async fn user_turn(
    State(state): State<AppState>,
    Path(run_id): Path<RunId>,
    headers: HeaderMap,
    Json(sub): Json<UserTurnSubmission>,
) -> Result<StatusCode, HttpError> {
    let entry = run_entry(&state, &run_id)?;
    check_lease(&headers, &entry, &run_id)?;
    entry
        .state
        .lock()
        .await
        .submit_user_turn(sub)
        .map_err(|source| HttpError::Submission { source })?;
    Ok(StatusCode::ACCEPTED)
}

fn run_entry(state: &AppState, run_id: &RunId) -> Result<Arc<RunEntry>, HttpError> {
    state
        .runs
        .get(run_id)
        .map(|entry| Arc::clone(entry.value()))
        .ok_or_else(|| HttpError::RunNotFound { id: run_id.clone() })
}

fn check_lease(headers: &HeaderMap, entry: &RunEntry, run_id: &RunId) -> Result<(), HttpError> {
    use subtle::ConstantTimeEq;

    let value = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or(HttpError::MissingLease)?;
    let (scheme, token) = value.split_once(' ').ok_or(HttpError::MissingLease)?;
    if !scheme.eq_ignore_ascii_case("Bearer") || token.is_empty() {
        return Err(HttpError::MissingLease);
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
        Err(HttpError::InvalidLease { id: run_id.clone() })
    }
}

async fn score_completed_scenario(
    state: &AppState,
    entry: &RunEntry,
    scenario_id: &wyrd_spec::vala::eval::ScenarioId,
) -> Result<(), HttpError> {
    let Some(scoring) = state.scoring.as_ref() else {
        return Ok(());
    };
    // Peek first so the cursor survives a scoring failure. Only take (remove)
    // after both score and aggregation succeed — otherwise a failed score
    // would permanently lose the cursor and stall the run.
    let cursor = entry
        .state
        .lock()
        .await
        .peek_completed_scenario(scenario_id)
        .map_err(|source| HttpError::Engine { source })?;
    let result = scoring
        .score_scenario(&cursor)
        .await
        .map_err(|source| HttpError::Engine { source })?;
    let aggregation = scoring
        .scenario_aggregation(&cursor, &result)
        .map_err(|source| HttpError::Engine { source })?;
    entry
        .state
        .lock()
        .await
        .take_completed_scenario(scenario_id)
        .map_err(|source| HttpError::Engine { source })?;
    entry.scenario_results.lock().await.push(result);
    entry.scenario_aggregations.lock().await.push(aggregation);
    Ok(())
}

async fn finalize_run(state: &AppState, entry: &RunEntry) -> Result<(), HttpError> {
    let Some(scoring) = state.scoring.as_ref() else {
        return Ok(());
    };
    if entry.run_result.lock().await.is_some() {
        return Ok(());
    }
    let (run_id, eval_ref, started_at) = {
        let run = entry.state.lock().await;
        (run.run_id.clone(), run.eval_ref.clone(), run.opened_at)
    };
    let scenarios = entry.scenario_aggregations.lock().await.clone();
    let result = scoring
        .finalize(
            RunIdentity {
                run_id: run_id.clone(),
                eval_ref,
                started_at,
                ended_at: Utc::now(),
            },
            scenarios,
        )
        .map_err(|source| HttpError::Engine { source })?;
    *entry.run_result.lock().await = Some(result);
    // Evict the completed run entry; results are now in the caller's hands.
    state.runs.remove(&run_id);
    Ok(())
}
