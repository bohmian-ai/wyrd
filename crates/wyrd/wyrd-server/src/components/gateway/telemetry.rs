//! Monotonic latency telemetry of governed gateway calls.
//!
//! A public gateway request is timed from its [`RequestArrival`], before
//! authentication and body decode. [`IngressClock`] carries the arrival and
//! the authentication interval from the ingress middleware to the handler,
//! which records the `authenticate` and `parse` phases once the request is
//! decoded and validated. [`CallTelemetry`] then owns the call's metric
//! lifetime through the invocation: every later phase, and at each handoff
//! point the call's total server duration, the provider exchange time it
//! waited on, and their difference, the server overhead.
//!
//! Points are handoffs to the response body: `answer` when a buffered answer
//! or a stream's head is handed over, and `end` when the stream's final frame
//! is taken by its body consumer. Neither proves the peer received anything.
//! Provider time comes from the exchanges Skald logged (see
//! [`wyrd_gateway::ExecutionTiming`]), so credential, translation, and decode
//! work stays server overhead. A call without an arrival, such as one a
//! Verifier makes, records phases only. Every label is closed; nothing here
//! is persisted, accounted, or sent on the wire.

use std::time::{Duration, Instant};

use wyrd_gateway::outcome_name;
use wyrd_spec::gateway::{GatewayCallOutcome, GatewayOperation};

use super::capture::operation_name;
use crate::app::metrics::{
    GATEWAY_DURATION_SECONDS, GATEWAY_OVERHEAD_SECONDS, GATEWAY_PHASE_DURATION_SECONDS,
    GATEWAY_UPSTREAM_SECONDS,
};
use crate::http::middleware::metrics::RequestArrival;

/// One request-path phase of a governed call, as its `phase` label names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    /// Credential extraction and verification or API-key exchange.
    Authenticate,
    /// Body read and decode, typed validation, and request-header parsing.
    Parse,
    /// The tenant gateway snapshot load.
    Snapshot,
    /// The requested model's invoke decision and Card attribution.
    Authorize,
    /// Candidate planning, fallback authorization, and representability.
    Route,
    /// The admission transaction.
    Admit,
    /// Provider credential resolution, summed across attempts.
    Credential,
    /// Adapter work outside provider exchanges, summed across attempts.
    Translate,
    /// The settlement statement of a call a limit or budget covered; zero
    /// for a call that charged nothing.
    Account,
    /// Relay decoding, translation, and framing of a stream's chunks.
    Relay,
    /// Capture projection and staging, after the answer.
    Capture,
}

impl Phase {
    /// The closed `phase` label.
    const fn name(self) -> &'static str {
        match self {
            Self::Authenticate => "authenticate",
            Self::Parse => "parse",
            Self::Snapshot => "snapshot",
            Self::Authorize => "authorize",
            Self::Route => "route",
            Self::Admit => "admit",
            Self::Credential => "credential",
            Self::Translate => "translate",
            Self::Account => "account",
            Self::Relay => "relay",
            Self::Capture => "capture",
        }
    }
}

/// Request-path timing the gateway ingress middleware hands to its handler.
#[derive(Debug, Clone, Copy)]
pub(crate) struct IngressClock {
    /// When the request reached the server.
    arrival: Instant,
    /// When authentication started.
    authenticating: Instant,
    /// When authentication finished.
    authenticated: Instant,
}

impl IngressClock {
    /// The clock of a request authenticated from `authenticating` until now,
    /// arriving at `arrival`, or at `authenticating` when the arrival
    /// middleware did not run.
    pub(crate) fn authenticated(arrival: Option<RequestArrival>, authenticating: Instant) -> Self {
        Self {
            arrival: arrival.map_or(authenticating, RequestArrival::instant),
            authenticating,
            authenticated: Instant::now(),
        }
    }

    /// Records the `authenticate` phase and the `parse` phase, which ends
    /// now, for `operation`, and returns the arrival.
    pub(crate) fn parsed(self, operation: GatewayOperation) -> Instant {
        let telemetry = CallTelemetry::new(operation, false, None);
        telemetry.phase(
            Phase::Authenticate,
            self.authenticated
                .saturating_duration_since(self.authenticating),
        );
        telemetry.phase(Phase::Parse, self.authenticated.elapsed());
        self.arrival
    }
}

/// The metric lifetime of one governed call.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CallTelemetry {
    /// Closed operation label.
    operation: &'static str,
    /// Whether the caller asked for a stream.
    stream: bool,
    /// When the request reached the server; `None` records phases only.
    arrival: Option<Instant>,
}

impl CallTelemetry {
    /// Telemetry of one `operation` call, streamed when `stream`, that
    /// arrived at `arrival`.
    pub(crate) const fn new(
        operation: GatewayOperation,
        stream: bool,
        arrival: Option<Instant>,
    ) -> Self {
        Self {
            operation: operation_name(operation),
            stream,
            arrival,
        }
    }

    /// Records `elapsed` as one observation of `phase`.
    pub(crate) fn phase(&self, phase: Phase, elapsed: Duration) {
        metrics::histogram!(
            GATEWAY_PHASE_DURATION_SECONDS,
            "operation" => self.operation,
            "phase" => phase.name()
        )
        .record(elapsed.as_secs_f64());
    }

    /// Records the time from `started` until now as one observation of
    /// `phase`.
    pub(crate) fn since(&self, phase: Phase, started: Instant) {
        self.phase(phase, started.elapsed());
    }

    /// Records the `answer` point now: a buffered answer, a refusal, or a
    /// stream's head handed to the response body with `outcome`, after
    /// waiting `upstream` on provider exchanges (until the stream's headers,
    /// for a stream).
    pub(crate) fn answered(&self, outcome: GatewayCallOutcome, upstream: Duration) {
        let point = if self.stream { "headers" } else { "end" };
        self.point("answer", outcome, Instant::now(), upstream, point);
    }

    /// Records the `end` point of a stream whose final frame its consumer
    /// took at `handed`, with `outcome`, after waiting `upstream` on provider
    /// exchanges until the stream's last byte.
    pub(crate) fn ended(&self, outcome: GatewayCallOutcome, handed: Instant, upstream: Duration) {
        self.point("end", outcome, handed, upstream, "end");
    }

    /// Records the server duration from arrival to `at`, the `upstream`
    /// time under `upstream_point`, and their difference at handoff `point`;
    /// nothing without an arrival.
    ///
    /// The same duration and provider wait are also recorded through
    /// [`wyrd_telemetry::record_timing`] under the current span's benchmark
    /// classification, at boundary `gateway.buffered`,
    /// `gateway.stream.head`, or `gateway.stream.end`; the benchmark
    /// histograms keep each call's residual beside both.
    fn point(
        &self,
        point: &'static str,
        outcome: GatewayCallOutcome,
        at: Instant,
        upstream: Duration,
        upstream_point: &'static str,
    ) {
        let Some(arrival) = self.arrival else {
            return;
        };
        let duration = at.saturating_duration_since(arrival);
        let stream = if self.stream { "true" } else { "false" };
        let outcome = outcome_name(outcome);
        wyrd_telemetry::record_timing(
            &tracing::Span::current(),
            match (self.stream, point) {
                (false, _) => "gateway.buffered",
                (true, "answer") => "gateway.stream.head",
                (true, _) => "gateway.stream.end",
            },
            outcome,
            duration,
            Some(upstream),
        );
        metrics::histogram!(
            GATEWAY_DURATION_SECONDS,
            "operation" => self.operation,
            "stream" => stream,
            "point" => point,
            "outcome" => outcome
        )
        .record(duration.as_secs_f64());
        metrics::histogram!(
            GATEWAY_OVERHEAD_SECONDS,
            "operation" => self.operation,
            "stream" => stream,
            "point" => point,
            "outcome" => outcome
        )
        .record(duration.saturating_sub(upstream).as_secs_f64());
        metrics::histogram!(
            GATEWAY_UPSTREAM_SECONDS,
            "operation" => self.operation,
            "stream" => stream,
            "point" => upstream_point
        )
        .record(upstream.as_secs_f64());
    }
}

#[cfg(test)]
mod tests {
    //! Proof of the handoff boundaries' benchmark histogram observations.

    use super::*;
    use wyrd_telemetry::timing;

    /// A stream records its head and its final frame as separate
    /// observations, each with its own provider wait and residual; a
    /// buffered call whose wait exceeds its time is a negative residual,
    /// never a clamped zero; a call without an arrival records nothing.
    ///
    /// # Panics
    /// Panics when a boundary, count, wait, or residual is wrong.
    #[test]
    fn histograms_preserve_head_and_final_overhead() {
        timing::enable();
        let before = timing::snapshot();
        let arrival = Instant::now()
            .checked_sub(Duration::from_millis(300))
            .expect("invariant: the monotonic clock is past 300 ms");
        let stream = CallTelemetry::new(GatewayOperation::ChatCompletions, true, Some(arrival));
        stream.answered(GatewayCallOutcome::Succeeded, Duration::from_millis(100));
        stream.ended(
            GatewayCallOutcome::Succeeded,
            arrival + Duration::from_millis(500),
            Duration::from_millis(450),
        );
        CallTelemetry::new(GatewayOperation::ChatCompletions, false, Some(arrival))
            .answered(GatewayCallOutcome::Succeeded, Duration::from_secs(1));
        CallTelemetry::new(GatewayOperation::ChatCompletions, false, None)
            .answered(GatewayCallOutcome::Succeeded, Duration::ZERO);
        let interval = timing::snapshot().since(&before).expect("one process");
        let series = |boundary: &str| {
            interval
                .series
                .iter()
                .find(|series| series.boundary == boundary)
                .unwrap_or_else(|| panic!("no {boundary} series: {:?}", interval.series))
        };
        let holds = |distribution: Option<&timing::Distribution>, ns: u64| {
            distribution
                .and_then(timing::Distribution::maximum)
                .is_some_and(|bucket| bucket.lowest <= ns && ns <= bucket.highest)
        };
        let head = series("gateway.stream.head");
        assert_eq!((head.outcome.as_str(), head.count), ("succeeded", 1));
        assert!(head.problems().is_empty(), "{:?}", head.problems());
        assert!(holds(head.excluded.as_ref(), 100_000_000));
        let end = series("gateway.stream.end");
        assert!(end.problems().is_empty(), "{:?}", end.problems());
        assert!(
            holds(Some(&end.elapsed), 500_000_000),
            "handoff to the final frame"
        );
        assert!(holds(end.excluded.as_ref(), 450_000_000));
        assert!(holds(end.residual.as_ref(), 50_000_000), "its own overhead");
        let buffered = series("gateway.buffered");
        assert_eq!((buffered.count, buffered.negative_residual), (1, 1));
        assert!(
            !buffered.problems().is_empty(),
            "a negative residual is invalid"
        );
        assert_eq!(
            interval.series.len(),
            3,
            "an unarrived call records nothing"
        );
    }
}
