//! Unary batch bounds.

use crate::resources::BifrostResourceError;
use num_traits::ToPrimitive;

/// Default largest single ingest request Scribe plans and reserves for.
///
/// Scribe reserves a crash-replayable envelope for one request of this size and
/// refuses to boot when the node cannot cover it, so raising it raises the
/// memory a node needs to start. Operators override it with
/// `scribe.ingest_request_bytes`; the ingest HTTP body and ingest gRPC
/// decoded-message ceilings both equal it exactly.
pub const BIFROST_INGEST_REQUEST_LIMIT_BYTES: usize = 16 * 1024 * 1024;
/// Multiplier deriving the per-request expanded-data ceiling from the wire ceiling.
///
/// Expanded data is the Arrow value, offset, and validity buffers one request's
/// canonical rows retain plus the Scribe-managed physical columns, and for OTLP
/// the generated typed-request backing. Every ingest decoder refuses a request
/// whose expanded data exceeds this multiple of its wire ceiling before WAL.
pub const BIFROST_INGEST_EXPANSION_FACTOR: usize = 4;
/// Largest recursive OTLP `AnyValue` nesting depth any decoder accepts.
///
/// A fixed structural parser bound, not a memory ceiling.
pub const BIFROST_OTLP_VALUE_DEPTH_LIMIT: usize = 8;
/// Fixed WAL header and digest scratch maximum.
pub const BIFROST_WAL_WORKSPACE_LIMIT_BYTES: usize = 4 * 1024;

/// Immutable V1 OTLP limits shared by the server adapter and Scribe planner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OtlpWireLimits {
    /// Largest encoded protobuf or JSON request accepted by an adapter.
    pub request_bytes: usize,
    /// Largest recursive `AnyValue` nesting depth.
    pub value_depth: usize,
    /// Largest number of distinct time partitions one source may span.
    ///
    /// A source is split before WAL into at most this many distinct
    /// [`TimePartition`](crate::catalog::layout::TimePartition) values, and the
    /// material plan reserves durable slice metadata for that ceiling.
    pub time_partitions: usize,
}

/// Canonical OTLP V1 defaults used to initialize decode and projection limits.
pub const OTLP_WIRE_LIMITS: OtlpWireLimits = OtlpWireLimits {
    request_bytes: BIFROST_INGEST_REQUEST_LIMIT_BYTES,
    value_depth: BIFROST_OTLP_VALUE_DEPTH_LIMIT,
    time_partitions: 32,
};

impl OtlpWireLimits {
    /// Returns the per-request expanded-data ceiling derived from the wire ceiling.
    ///
    /// Boot configuration rejects a wire ceiling whose multiplication overflows,
    /// so saturation is unreachable for a validated limit.
    #[must_use]
    pub const fn expanded_bytes(&self) -> usize {
        self.request_bytes
            .saturating_mul(BIFROST_INGEST_EXPANSION_FACTOR)
    }
}

/// Byte-weighted admission for encoded HTTP and tonic bodies.
///
/// Transport bodies are Bifrost memory: each lease charges its exact encoded
/// bytes to the one shared cap Scribe, Oracle, and Forge also charge, and
/// returns them when the body is consumed, dropped, cancelled, or refused by
/// decode. The per-message ceiling is a separate, boot-frozen bound checked
/// before anything is retained.
#[derive(Debug, Clone)]
pub struct BifrostTransportAdmission {
    /// The one process governor every transport charge lands in.
    governor: crate::resources::BifrostResourceGovernor,
    /// Boot-frozen maximum for one declared or decoded message.
    message_limit: usize,
}

impl BifrostTransportAdmission {
    /// Constructs transport admission over the shared process governor.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::InvalidPlan`] when the selected maximum
    /// is zero or cannot fit once inside the shared Bifrost cap.
    pub(crate) fn new(
        governor: crate::resources::BifrostResourceGovernor,
        message_limit_bytes: usize,
    ) -> Result<Self, BifrostResourceError> {
        let cap = governor.plan().managed_memory_bytes;
        if message_limit_bytes == 0 || message_limit_bytes > cap {
            return Err(BifrostResourceError::InvalidPlan {
                detail: format!(
                    "transport message limit {message_limit_bytes} must be positive and fit \
                     the {cap}-byte shared Bifrost cap"
                ),
            });
        }
        Ok(Self {
            governor,
            message_limit: message_limit_bytes,
        })
    }

    /// Builds a live admission object for tests that need one without a plan.
    ///
    /// Every serving path derives admission from the resource composition, so
    /// this exists only so unit tests and test shells can exercise byte-weighted
    /// admission without a resource observation. The private shared cap holds
    /// exactly two default maximum ingest messages, which is what the boundary
    /// tests assert against; it is not a production budget.
    ///
    /// # Panics
    ///
    /// Panics when the fixed test observation stops composing, which can only
    /// happen if the constants below are edited inconsistently.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn for_tests() -> Self {
        crate::resources::BifrostRuntimeResources::composed_for_test(
            2 * BIFROST_INGEST_REQUEST_LIMIT_BYTES,
            u64::MAX / 4,
            [],
        )
        .transport_admission()
    }

    /// Validates one message against the ceiling, then charges its exact bytes.
    ///
    /// Callers pass the encoded length they are about to retain: a declared
    /// `Content-Length`, a gRPC frame length, or the collected length of a
    /// body whose size was not declared. Nothing is precharged.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Occupied`] when the message exceeds the
    /// boot-selected individual cap or the shared Bifrost cap cannot cover it,
    /// and a poison error for an untrustworthy ledger.
    pub fn try_acquire(&self, bytes: usize) -> Result<BifrostTransportLease, BifrostResourceError> {
        if bytes > self.message_limit {
            record_transport("refused_message_limit", self.used_bytes());
            return Err(BifrostResourceError::Occupied {
                detail: format!(
                    "transport message exceeds the {} byte encoded-body limit",
                    self.message_limit
                ),
            });
        }
        if let Err(error) = self.governor.try_charge_transport(bytes) {
            record_transport("refused_occupied", self.used_bytes());
            return Err(error);
        }
        record_transport("acquired", self.used_bytes());
        Ok(BifrostTransportLease {
            bytes,
            admission: self.clone(),
        })
    }

    /// Returns exact live transport ownership for telemetry and tests.
    #[must_use]
    pub fn used_bytes(&self) -> usize {
        self.governor.transport_used_bytes()
    }

    /// Returns the shared Bifrost cap transport competes for.
    #[must_use]
    pub fn limit_bytes(&self) -> usize {
        self.governor.plan().managed_memory_bytes
    }

    /// Returns the boot-frozen maximum for one encoded message.
    #[must_use]
    pub const fn message_limit_bytes(&self) -> usize {
        self.message_limit
    }
}

/// Exact RAII ownership for one encoded transport body.
#[derive(Debug)]
pub struct BifrostTransportLease {
    /// Exact encoded bytes charged to the shared cap.
    bytes: usize,
    /// Shared process admission that receives cancellation and terminal drops.
    admission: BifrostTransportAdmission,
}

impl BifrostTransportLease {
    /// Returns the exact byte charge retained by this lease.
    #[must_use]
    pub const fn bytes(&self) -> usize {
        self.bytes
    }
}

impl Drop for BifrostTransportLease {
    /// Releases exact ownership on success, error, or cancellation.
    fn drop(&mut self) {
        if let Err(error) = self.admission.governor.release_transport(self.bytes) {
            tracing::error!(%error, "transport lease release could not be reconciled");
        }
        record_transport("released", self.admission.used_bytes());
    }
}

/// Emits bounded transport lifecycle metrics after each atomic transition.
fn record_transport(result: &'static str, current_bytes: usize) {
    metrics::counter!(
        "bifrost_resource_acquisitions_total",
        "role" => "transport",
        "resource" => "memory",
        "result" => result
    )
    .increment(1);
    metrics::gauge!(
        "bifrost_resource_current_bytes",
        "role" => "transport",
        "resource" => "memory"
    )
    .set(current_bytes.to_f64().unwrap_or(f64::MAX));
}

/// Hard bounds enforced before a batch enters Scribe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IngestLimits {
    /// Operator-selected wire ceiling for one ingest request.
    ///
    /// It is also the ingest gRPC decoded-message ceiling and the ingest HTTP
    /// body ceiling; no framing allowance is added.
    pub max_frame_bytes: usize,
    /// Immutable OTLP wire and structural shape limits.
    pub otlp: OtlpWireLimits,
    /// Fixed WAL header and digest scratch retained by one ingress root.
    pub wal_workspace_bytes: usize,
}

impl IngestLimits {
    /// Returns the per-request expanded-data ceiling derived from the wire ceiling.
    ///
    /// Boot configuration rejects a wire ceiling whose multiplication overflows,
    /// so saturation is unreachable for a validated limit.
    #[must_use]
    pub const fn expanded_bytes(&self) -> usize {
        self.max_frame_bytes
            .saturating_mul(BIFROST_INGEST_EXPANSION_FACTOR)
    }
}

impl Default for IngestLimits {
    fn default() -> Self {
        Self {
            max_frame_bytes: BIFROST_INGEST_REQUEST_LIMIT_BYTES,
            otlp: OTLP_WIRE_LIMITS,
            wal_workspace_bytes: BIFROST_WAL_WORKSPACE_LIMIT_BYTES,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Transport charges exact encoded bytes against the shared cap and
    /// releases them on drop; the per-message ceiling is checked first.
    ///
    /// # Panics
    ///
    /// Panics when an exact-boundary acquisition unexpectedly fails.
    #[test]
    fn bifrost_transport_admission_charges_exact_bytes_to_the_shared_cap() {
        let admission = BifrostTransportAdmission::for_tests();
        let first = admission
            .try_acquire(BIFROST_INGEST_REQUEST_LIMIT_BYTES)
            .expect("first maximum body");
        let second = admission
            .try_acquire(BIFROST_INGEST_REQUEST_LIMIT_BYTES)
            .expect("equal cap boundary succeeds");
        assert!(admission.try_acquire(1).is_err(), "the shared cap is full");
        drop(first);
        let small = admission.try_acquire(1).expect("one exact byte");
        assert_eq!(small.bytes(), 1, "no quantum or maximum-message precharge");
        assert_eq!(
            admission.used_bytes(),
            BIFROST_INGEST_REQUEST_LIMIT_BYTES + 1
        );
        drop(small);
        drop(second);
        assert_eq!(admission.used_bytes(), 0);
        assert!(
            admission
                .try_acquire(BIFROST_INGEST_REQUEST_LIMIT_BYTES + 1)
                .is_err(),
            "the per-message ceiling is independent of free capacity"
        );
        assert_eq!(admission.used_bytes(), 0, "refusal charges nothing");
    }
}
