//! Closed-label peer, fragment, slot, and security telemetry.

use std::time::Instant;
use wyrd_spec::vala::api::QueryClass;

/// Closed locality label for one fragment attempt.
#[derive(Debug, Clone, Copy)]
pub enum FragmentLocality {
    /// Attempt executed on the leader node.
    Local,
    /// Attempt executed through a peer transport.
    Remote,
}

impl FragmentLocality {
    /// Returns the canonical low-cardinality metric label.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
        }
    }
}

/// Closed terminal fragment outcome.
#[derive(Debug, Clone, Copy)]
pub enum FragmentOutcome {
    /// Footer-validated bytes were admitted.
    Success,
    /// The attempt failed or was cancelled.
    Failed,
}

impl FragmentOutcome {
    /// Returns the canonical low-cardinality metric label.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failed => "failed",
        }
    }
}

/// Closed peer-attempt error class.
#[derive(Debug, Clone, Copy)]
pub enum PeerErrorClass {
    /// No error occurred.
    None,
    /// Transport or worker availability failed.
    Availability,
    /// Security or closed-contract validation failed.
    Security,
    /// All bounded distinct attempts failed.
    Exhausted,
    /// Footer identity validation failed.
    Footer,
    /// Whole-attempt buffering or footer completeness failed.
    Attempt,
}

impl PeerErrorClass {
    /// Returns the canonical low-cardinality metric label.
    const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Availability => "availability",
            Self::Security => "security",
            Self::Exhausted => "exhausted",
            Self::Footer => "footer",
            Self::Attempt => "attempt",
        }
    }
}

/// Closed worker-slot reservation outcome.
#[derive(Debug, Clone, Copy)]
pub enum SlotOutcome {
    /// A pending slot was accepted.
    Pending,
    /// A pending reservation transitioned to running.
    Running,
    /// Capacity or fencing rejected the reservation.
    Rejected,
}

impl SlotOutcome {
    /// Returns the canonical low-cardinality metric label.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Rejected => "rejected",
        }
    }
}

/// Closed peer-security event boundary.
#[derive(Debug, Clone, Copy)]
pub enum SecurityEventClass {
    /// Raw ticket authentication failed.
    Ticket,
    /// Verified claim structure failed.
    Claims,
    /// Fragment-bound verified claims failed.
    Fragment,
}

impl SecurityEventClass {
    /// Returns the canonical low-cardinality metric label.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Ticket => "ticket",
            Self::Claims => "claims",
            Self::Fragment => "fragment",
        }
    }
}

/// Stream-scoped fragment metrics released on every exit path.
pub struct FragmentTelemetry {
    /// Closed locality label.
    locality: FragmentLocality,
    /// Fragment start used by the canonical duration histogram.
    started_at: Instant,
    /// Whether a terminal outcome has already been recorded.
    finished: bool,
}

impl FragmentTelemetry {
    /// Starts canonical in-flight and duration accounting.
    #[must_use]
    pub fn start(locality: FragmentLocality) -> Self {
        metrics::gauge!(
            "bifrost_oracle_fragments_in_flight",
            "locality" => locality.as_str()
        )
        .increment(1.0);
        Self {
            locality,
            started_at: Instant::now(),
            finished: false,
        }
    }

    /// Records one terminal fragment outcome and admitted encoded bytes.
    pub fn finish(&mut self, outcome: FragmentOutcome, encoded_bytes: u64) {
        if self.finished {
            return;
        }
        self.finished = true;
        metrics::counter!(
            "bifrost_oracle_fragments_total",
            "outcome" => outcome.as_str(),
            "locality" => self.locality.as_str()
        )
        .increment(1);
        metrics::histogram!(
            "bifrost_oracle_fragment_seconds",
            "outcome" => outcome.as_str(),
            "locality" => self.locality.as_str()
        )
        .record(self.started_at.elapsed().as_secs_f64());
        metrics::counter!(
            "bifrost_oracle_fragment_bytes_total",
            "locality" => self.locality.as_str()
        )
        .increment(encoded_bytes);
    }
}

impl Drop for FragmentTelemetry {
    /// Records cancellation/failure and closes the in-flight gauge.
    fn drop(&mut self) {
        if !self.finished {
            self.finish(FragmentOutcome::Failed, 0);
        }
        metrics::gauge!(
            "bifrost_oracle_fragments_in_flight",
            "locality" => self.locality.as_str()
        )
        .decrement(1.0);
    }
}

/// Records one peer attempt outcome with closed labels.
pub fn record_peer_attempt(outcome: FragmentOutcome, error_class: PeerErrorClass) {
    metrics::counter!(
        "bifrost_oracle_peer_attempts_total",
        "outcome" => outcome.as_str(),
        "error_class" => error_class.as_str()
    )
    .increment(1);
}

/// Records one pending/running worker-slot reservation outcome.
pub fn record_slot(query_class: QueryClass, outcome: SlotOutcome) {
    metrics::counter!(
        "bifrost_oracle_slot_reservations_total",
        "role" => "worker",
        "query_class" => match query_class {
            QueryClass::Interactive => "interactive",
            QueryClass::Analytical => "analytical",
        },
        "outcome" => outcome.as_str()
    )
    .increment(1);
}

/// Records one closed peer-security event class.
pub fn record_security(event_class: SecurityEventClass) {
    metrics::counter!(
        "bifrost_oracle_security_events_total",
        "event_class" => event_class.as_str()
    )
    .increment(1);
}
