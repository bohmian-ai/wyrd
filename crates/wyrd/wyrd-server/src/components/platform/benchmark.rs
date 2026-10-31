//! The benchmark Scribe control: stage and publish on request.
//!
//! Compiled only with the `bench` feature, which only the benchmark build
//! enables, so a shipped server has neither path. A benchmark moves its fixture
//! between the memtable, staged runs and published objects deliberately with
//! these two routes instead of waiting on rotation or retention, so what it
//! measures does not depend on timing.
//!
//! Both act on every tenant this replica holds, so both require the platform
//! administrator's `tenant_suspend` permission, the existing permission with
//! that reach, and each decision is audited like any other platform decision.
//! Neither is an OpenAPI operation, and neither has a compatibility promise.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::routing::post;
use tokio::sync::Mutex;
use utoipa_axum::router::OpenApiRouter;
use vala_bifrost_redux::contracts::ScribeError;
use vala_bifrost_redux::scribe::ScribeImpl;
use wyrd_auth::platform_authz::{PLATFORM_TENANTS_RESOURCE, PlatformAuthorization};
use wyrd_runtime::Permission;
use wyrd_spec::error::WyrdError;

use crate::components::auth::PlatformCaller;
use crate::components::platform::identity::{authorize, commit_decision, operator};
use crate::http::error::{WyrdErrorResponse, internal_failure};
use crate::state::AppState;

/// Serializes control requests on this replica: a second waits for the first.
static CONTROL: Mutex<()> = Mutex::const_new(());

/// Build the benchmark control routes.
pub fn platform_benchmark_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .route("/platform/benchmark/scribe/stage", post(stage))
        .route("/platform/benchmark/scribe/publish", post(publish))
}

/// Stage every writable row this replica holds, publishing nothing.
///
/// Answers `{"rows_staged": n}` once every row acknowledged before the request
/// is in a durable, query-registered staged run.
///
/// # Errors
/// Returns 401 without a platform session, 403 without `tenant_suspend`, and a
/// stable internal error when this replica has no Scribe or a shard cannot
/// stage; staged shards stay staged and a retry stages the rest.
#[tracing::instrument(level = "info", skip_all)]
async fn stage(
    State(state): State<AppState>,
    caller: PlatformCaller,
) -> Result<Json<serde_json::Value>, WyrdErrorResponse> {
    let rows = control(&state, &caller, |scribe| async move {
        scribe.stage_writable().await
    })
    .await?;
    Ok(Json(serde_json::json!({ "rows_staged": rows })))
}

/// Stage, claim and publish every row this replica holds.
///
/// Runs the production stage, claim and publish path, so each claim installs
/// its learned layout as it does on rotation. Answers
/// `{"claims_published": n}`.
///
/// # Errors
/// As [`stage`]; claims already published stay published and a retry
/// publishes the rest.
#[tracing::instrument(level = "info", skip_all)]
async fn publish(
    State(state): State<AppState>,
    caller: PlatformCaller,
) -> Result<Json<serde_json::Value>, WyrdErrorResponse> {
    let claims = control(&state, &caller, |scribe| async move {
        scribe.flush_staged().await
    })
    .await?;
    Ok(Json(serde_json::json!({ "claims_published": claims })))
}

/// Authorize, audit and run one Scribe transition, one request at a time.
///
/// The transition runs on its own task, so a dropped connection never cancels
/// one already started.
///
/// # Errors
/// Returns the authorization error, or an internal error when this replica has
/// no Scribe or the transition fails.
async fn control<F, Fut>(
    state: &AppState,
    caller: &PlatformCaller,
    transition: F,
) -> Result<usize, WyrdErrorResponse>
where
    F: FnOnce(Arc<ScribeImpl>) -> Fut,
    Fut: Future<Output = Result<usize, ScribeError>> + Send + 'static,
{
    let pool = operator(state)?;
    // The handle must outlive the transaction it lends out.
    let authz =
        PlatformAuthorization::new(pool.clone(), Arc::clone(state.bifrost.scribe_outbox()) as _);
    let decision = authorize(
        &authz,
        caller,
        &Permission::tenant_suspend(),
        PLATFORM_TENANTS_RESOURCE,
    )
    .await?;
    commit_decision(decision).await?;
    let Some(runtime) = state.bifrost.scribe() else {
        return Err(WyrdError::Internal {
            message: "this replica runs no Scribe".to_owned(),
            details: serde_json::json!({ "role": "scribe" }),
        }
        .into());
    };
    let scribe = Arc::clone(runtime.scribe());
    let _turn = CONTROL.lock().await;
    tokio::spawn(transition(scribe))
        .await
        .map_err(|error| internal_failure("the Scribe transition stopped", &error))?
        .map_err(|error| internal_failure("the Scribe transition failed", &error).into())
}
