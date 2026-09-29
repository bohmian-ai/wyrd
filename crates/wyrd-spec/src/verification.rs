//! Wire contracts of the Verification control-plane API.
//!
//! Three HTTP operations — `GET /v1/verification/bindings/{binding_id}`,
//! `POST /v1/verification/runs`, and `GET /v1/verification/runs/{run_id}` —
//! and their Rust, Python, TypeScript, and MCP projections share these typed
//! shapes. They carry durable control state owned by Postgres: binding
//! activity and readiness, run execution status, the result pointer, and each
//! Operator dispatch's delivery status. Verdicts and Drift/Eval details stay in
//! Bifrost and are never copied here.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::PrincipalId;
use crate::error::WyrdError;
use crate::ids::{BindingId, CardUid, OperatorDispatchId, VerificationResultId, VerificationRunId};

/// Longest manual Drift window a caller may request: 31 days.
///
/// One calendar month covers every daily, weekly, and monthly cron window a
/// scheduled binding can produce, so a caller can re-run any such occurrence
/// by hand, while the bound caps the Bifrost scan one request can trigger.
pub const MAX_MANUAL_DRIFT_WINDOW_DAYS: i64 = 31;

/// A frozen reference to one effective Trigger or Operator.
///
/// A referenced Card is frozen by its exact UID; an inline body, which has no
/// Card, by the canonical digest of its typed spec. Binding projection, runs,
/// and dispatches store this identity so later Card changes never retarget
/// admitted work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum FrozenTarget {
    /// A registered Card version.
    Uid(CardUid),
    /// The canonical spec digest of an inline body.
    Digest(String),
}

/// A half-open UTC Drift comparison window `[start, end)`.
///
/// Scheduled runs derive it from the due cron occurrence; manual runs supply
/// it and validate it with [`DriftWindow::validate_manual`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct DriftWindow {
    /// Inclusive window start.
    pub start: DateTime<Utc>,
    /// Exclusive window end.
    pub end: DateTime<Utc>,
}

impl DriftWindow {
    /// Validate a caller-supplied manual window.
    ///
    /// The window must be non-empty (`start < end`) and no longer than
    /// [`MAX_MANUAL_DRIFT_WINDOW_DAYS`]. Timestamps with any offset are
    /// accepted on the wire and normalized to UTC during deserialization, so
    /// every stored window is UTC.
    ///
    /// # Errors
    /// Returns [`WyrdError::VerificationInvalidWindow`] for an empty,
    /// inverted, or over-long window.
    pub fn validate_manual(&self) -> Result<(), WyrdError> {
        if self.start >= self.end {
            return Err(WyrdError::VerificationInvalidWindow {
                message: "drift_window start must be before end".to_owned(),
                details: serde_json::json!({ "start": self.start, "end": self.end }),
            });
        }
        if self.end - self.start > Duration::days(MAX_MANUAL_DRIFT_WINDOW_DAYS) {
            return Err(WyrdError::VerificationInvalidWindow {
                message: format!(
                    "drift_window may span at most {MAX_MANUAL_DRIFT_WINDOW_DAYS} days"
                ),
                details: serde_json::json!({
                    "start": self.start,
                    "end": self.end,
                    "max_days": MAX_MANUAL_DRIFT_WINDOW_DAYS,
                }),
            });
        }
        Ok(())
    }
}

/// What a manual run verifies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VerificationRunTarget {
    /// One projected binding; its frozen owner, Trigger, and `on_failure`
    /// Operators apply, so a failed verdict dispatches them.
    Binding {
        /// The binding to run.
        binding_id: BindingId,
    },
    /// A direct, analysis-only run of one exact Verifier over one subject.
    /// It has no owner, binding, Trigger, or Operator and never dispatches.
    Verifier {
        /// Exact Verifier Card version.
        verifier_uid: CardUid,
        /// Exact subject Card whose observations are analyzed.
        subject_card_uid: CardUid,
    },
}

/// The input a manual run analyzes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VerificationRunInput {
    /// A bounded half-open UTC Drift window `[start, end)`.
    DriftWindow {
        /// Inclusive window start.
        start: DateTime<Utc>,
        /// Exclusive window end.
        end: DateTime<Utc>,
    },
}

impl VerificationRunInput {
    /// Return the Drift window this input names.
    #[must_use]
    pub fn drift_window(&self) -> DriftWindow {
        match self {
            Self::DriftWindow { start, end } => DriftWindow {
                start: *start,
                end: *end,
            },
        }
    }
}

/// `POST /v1/verification/runs` request body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct StartVerificationRunRequest {
    /// Binding or direct Verifier target.
    pub target: VerificationRunTarget,
    /// Input to analyze.
    pub input: VerificationRunInput,
}

impl StartVerificationRunRequest {
    /// Validate everything decidable without IO and return the Drift window.
    ///
    /// Target existence, tenancy, authorization, and readiness are decided by
    /// the server against durable state; this checks only the window.
    ///
    /// # Errors
    /// Returns [`WyrdError::VerificationInvalidWindow`] when the window is
    /// empty, inverted, or longer than [`MAX_MANUAL_DRIFT_WINDOW_DAYS`].
    pub fn validate(&self) -> Result<DriftWindow, WyrdError> {
        let window = self.input.drift_window();
        window.validate_manual()?;
        Ok(window)
    }
}

/// `202 Accepted` response to `POST /v1/verification/runs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct StartVerificationRunResponse {
    /// The durably enqueued run; poll it with `GET /v1/verification/runs/{run_id}`.
    pub run_id: VerificationRunId,
}

/// Whether a Verifier may produce a judgment now.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    strum::EnumString,
    strum::IntoStaticStr,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum VerifierReadiness {
    /// The Verifier can run.
    Ready,
    /// A PSI or SPC Drift Verifier's fitted baseline is not ready yet.
    BaselineNotReady,
    /// The Verifier Card is missing, not a Verifier, or no longer active.
    VerifierUnavailable,
}

/// Execution state of one Verifier run.
///
/// Independent of the verdict: only `completed` has one, and it lives in the
/// Bifrost result the run points at. `cancelled`, `timed_out`, and `errored`
/// never carry a verdict or dispatch an Operator.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    strum::EnumString,
    strum::IntoStaticStr,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum VerificationExecutionStatus {
    /// Enqueued and waiting for its first claim.
    Pending,
    /// Claimed by a runner under an unexpired lease.
    Running,
    /// A retryable failure is waiting for its next attempt.
    Retrying,
    /// Executed and produced an acknowledged result.
    Completed,
    /// Stopped before producing a result.
    Cancelled,
    /// Exceeded its execution deadline.
    TimedOut,
    /// Failed terminally or exhausted its attempts.
    Errored,
}

/// Common Verifier verdict of a completed run.
///
/// Carried by the Bifrost result and used by settlement to decide Operator
/// dispatch; Run GET never returns it.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    strum::EnumString,
    strum::IntoStaticStr,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum VerificationVerdict {
    /// Expectations held.
    Passed,
    /// Expectations were violated; a binding-created run dispatches Operators.
    Failed,
    /// No judgment could be made; never a pass.
    Inconclusive,
}

/// Delivery state of one Operator dispatch.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    strum::EnumString,
    strum::IntoStaticStr,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum OperatorDispatchStatus {
    /// Created by settlement and waiting for its first claim.
    Pending,
    /// Claimed by the Operator worker under an unexpired lease.
    Running,
    /// A transient delivery failure is waiting for its next attempt.
    Retrying,
    /// The provider accepted the action.
    Delivered,
    /// Failed terminally or exhausted its attempts.
    Failed,
}

/// Structured, secret-free execution or delivery error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct VerificationError {
    /// Stable machine-readable error code.
    pub code: String,
    /// Bounded human-readable description.
    pub message: String,
}

/// `GET /v1/verification/bindings/{binding_id}` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct VerificationBindingStatus {
    /// The binding.
    pub binding_id: BindingId,
    /// Exact owner Service or standalone Agent Card version.
    pub owner_card_uid: CardUid,
    /// Exact verified subject Card.
    pub subject_card_uid: CardUid,
    /// Exact bound Verifier Card version.
    pub verifier_uid: CardUid,
    /// Whether the owner's Card-bound principal is runtime-active, so
    /// scheduled or observation work may be admitted now.
    pub active: bool,
    /// Whether the Verifier can run, and why not when it cannot.
    pub readiness: VerifierReadiness,
    /// Next scheduled occurrence; null before activation and always null for
    /// an observation-triggered (Eval) binding.
    pub next_run_at: Option<DateTime<Utc>>,
    /// The owner principal's last qualifying machine exchange.
    pub last_activated_at: Option<DateTime<Utc>>,
    /// Most recently enqueued run of this binding.
    pub last_run_id: Option<VerificationRunId>,
}

/// Current state of one Operator dispatch in a Run GET response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct OperatorDispatchState {
    /// The dispatch.
    pub dispatch_id: OperatorDispatchId,
    /// The frozen Operator Card UID or inline-spec digest.
    pub operator: FrozenTarget,
    /// Independent delivery status.
    pub status: OperatorDispatchStatus,
    /// Last delivery error, when one occurred.
    pub error: Option<VerificationError>,
}

/// `GET /v1/verification/runs/{run_id}` response.
///
/// Execution state and pointers only: the verdict and Drift/Eval details are
/// read from Bifrost by `result_id`. A run may point at an acknowledged result
/// before it is visible to analytical queries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct VerificationRunStatus {
    /// The run.
    pub run_id: VerificationRunId,
    /// Execution state.
    pub status: VerificationExecutionStatus,
    /// Authenticated caller of a manual run; null for scheduled and
    /// observation-created runs.
    pub requested_by_principal_id: Option<PrincipalId>,
    /// Canonical result of a completed run.
    pub result_id: Option<VerificationResultId>,
    /// Last execution error of a retrying or terminal run.
    pub error: Option<VerificationError>,
    /// Every Operator dispatch of a failed binding-created run, in identity
    /// order; empty otherwise.
    pub dispatches: Vec<OperatorDispatchState>,
}

#[cfg(test)]
mod tests {
    //! Synchronous wire validation of manual run requests.

    use chrono::TimeZone;

    use super::*;

    /// Build a UTC instant at `hour` on 2026-09-17.
    ///
    /// # Panics
    /// Panics when `hour` does not name exactly one instant.
    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 17, hour, 0, 0)
            .single()
            .expect("test_setup: unambiguous UTC instant")
    }

    /// The documented JSON shape parses into typed target and input, and a
    /// one-hour window validates.
    ///
    /// # Panics
    /// Panics when the documented request does not parse or validate.
    #[test]
    fn documented_request_parses_and_validates() {
        let binding = BindingId::new_v7();
        let request: StartVerificationRunRequest = serde_json::from_value(serde_json::json!({
            "target": { "kind": "binding", "binding_id": binding },
            "input": { "kind": "drift_window", "start": "2026-09-17T00:00:00Z", "end": "2026-09-17T01:00:00Z" }
        }))
        .expect("documented request parses");
        assert_eq!(
            request.target,
            VerificationRunTarget::Binding {
                binding_id: binding
            }
        );
        assert_eq!(
            request.validate().expect("valid window"),
            DriftWindow {
                start: at(0),
                end: at(1)
            }
        );
    }

    /// Offsets normalize to UTC, and empty, inverted, and over-long windows
    /// are refused with the stable window error.
    ///
    /// # Panics
    /// Panics when an offset is not normalized or an invalid window passes.
    #[test]
    fn window_validation_refuses_invalid_windows() {
        let input: VerificationRunInput = serde_json::from_value(serde_json::json!({
            "kind": "drift_window", "start": "2026-09-17T02:00:00+02:00", "end": "2026-09-17T01:00:00Z"
        }))
        .expect("offset timestamps parse");
        assert_eq!(input.drift_window().start, at(0));

        let limit = Duration::days(MAX_MANUAL_DRIFT_WINDOW_DAYS);
        let cases = [
            (at(1), at(1)),
            (at(2), at(1)),
            (at(0), at(0) + limit + Duration::seconds(1)),
        ];
        for (start, end) in cases {
            let error = DriftWindow { start, end }
                .validate_manual()
                .expect_err("invalid window is refused");
            assert_eq!(error.code(), "WYRD_VERIFICATION_400_INVALID_WINDOW");
        }
        assert!(
            DriftWindow {
                start: at(0),
                end: at(0) + limit
            }
            .validate_manual()
            .is_ok(),
            "the maximum span is inclusive"
        );
    }

    /// Unknown target kinds, unknown fields, and non-v7 identities fail to
    /// parse rather than reaching the server.
    ///
    /// # Panics
    /// Panics when a malformed target is accepted.
    #[test]
    fn malformed_targets_are_refused() {
        let v4 = uuid::Uuid::new_v4();
        for target in [
            serde_json::json!({ "kind": "service", "binding_id": BindingId::new_v7() }),
            serde_json::json!({ "kind": "binding", "binding_id": v4 }),
            serde_json::json!({ "kind": "binding", "binding_id": BindingId::new_v7(), "extra": 1 }),
        ] {
            assert!(serde_json::from_value::<VerificationRunTarget>(target).is_err());
        }
    }

    /// Status enums round-trip through their stored snake_case names.
    ///
    /// # Panics
    /// Panics when a stored name does not parse back to its variant.
    #[test]
    fn status_names_round_trip() {
        for status in [
            VerificationExecutionStatus::Pending,
            VerificationExecutionStatus::TimedOut,
            VerificationExecutionStatus::Errored,
        ] {
            let name: &'static str = status.into();
            assert_eq!(
                name.parse::<VerificationExecutionStatus>().ok(),
                Some(status)
            );
        }
        let readiness: &'static str = VerifierReadiness::BaselineNotReady.into();
        assert_eq!(readiness, "baseline_not_ready");
    }
}
