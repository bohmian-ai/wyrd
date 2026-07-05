//! `OrchestratorError → WyrdError` edge mapping for eval handlers.
//!
//! Eval handlers return the server's single `WyrdErrorResponse` like every other
//! route — there is no second `IntoResponse`. The shared
//! `wyrd_error_response_from_parts` mapper does **not** scrub 5xx, so the burden
//! of redacting engine/provider internals is entirely on this edge: any
//! `status >= 500` arm logs the source and returns a generic message with empty
//! `details`.

use std::fmt::Display;

use vala_eval::orchestrator::OrchestratorError;
use wyrd_spec::error::WyrdError;

/// Map a submission-time state-machine rejection to `WYRD_EVAL_409`.
///
/// Non-500: the mismatch description is a protocol-level message, not provider
/// internals, so it is surfaced to the client.
pub(crate) fn eval_submission_error(source: &OrchestratorError) -> WyrdError {
    WyrdError::EvalSubmissionMismatch {
        message: source.to_string(),
        details: serde_json::json!({}),
    }
}

/// Map an engine/orchestrator failure to a scrubbed `WYRD_EVAL_500`.
///
/// Logs the source and returns a generic message with empty `details` so
/// engine/provider internals never reach clients.
pub(crate) fn eval_engine_error(source: &OrchestratorError) -> WyrdError {
    tracing::error!(
        wyrd.error.code = "WYRD_EVAL_500_RUN_FAILED",
        wyrd.error.detail = %source,
        "eval engine error",
    );
    scrubbed_run_failed()
}

/// Map an internal server/config failure (scenario load, invariant) to a
/// scrubbed `WYRD_EVAL_500`. Logs `detail`; returns a generic body.
pub(crate) fn eval_internal_error(detail: impl Display) -> WyrdError {
    tracing::error!(
        wyrd.error.code = "WYRD_EVAL_500_RUN_FAILED",
        wyrd.error.detail = %detail,
        "eval run internal error",
    );
    scrubbed_run_failed()
}

/// Map a card-resolution `WyrdError` to the eval error surface, failing closed.
///
/// A missing/foreign card resolves to `registry_card_not_found` (404) via RLS —
/// mapped to `WYRD_EVAL_404_RUN_NOT_FOUND` so a cross-tenant reference is denied
/// with the same not-found code as a nonexistent run. Anything else (transient
/// DB, parse) is a scrubbed 500.
pub(crate) fn map_card_resolution_error(error: &WyrdError) -> WyrdError {
    if matches!(error, WyrdError::RegistryCardNotFound { .. }) {
        return eval_run_not_found();
    }
    tracing::error!(
        wyrd.error.code = "WYRD_EVAL_500_RUN_FAILED",
        wyrd.error.detail = %error,
        "eval card resolution failed",
    );
    scrubbed_run_failed()
}

/// `WYRD_EVAL_404_RUN_NOT_FOUND`.
pub(crate) fn eval_run_not_found() -> WyrdError {
    WyrdError::EvalRunNotFound {
        message: "eval run not found".to_owned(),
        details: serde_json::json!({}),
    }
}

/// `WYRD_EVAL_429_TOO_MANY_RUNS`.
pub(crate) fn eval_too_many_runs() -> WyrdError {
    WyrdError::EvalTooManyRuns {
        message: "too many concurrent eval runs; retry after an existing run completes".to_owned(),
        details: serde_json::json!({}),
    }
}

/// `WYRD_EVAL_401_MISSING_LEASE`.
pub(crate) fn eval_missing_lease() -> WyrdError {
    WyrdError::EvalMissingLease {
        message: "missing lease token on protected eval-run route".to_owned(),
        details: serde_json::json!({}),
    }
}

/// `WYRD_EVAL_403_INVALID_LEASE`.
pub(crate) fn eval_invalid_lease() -> WyrdError {
    WyrdError::EvalInvalidLease {
        message: "lease token does not match the lease issued for this run".to_owned(),
        details: serde_json::json!({}),
    }
}

fn scrubbed_run_failed() -> WyrdError {
    WyrdError::EvalRunFailed {
        message: "eval run failed".to_owned(),
        details: serde_json::json!({}),
    }
}
