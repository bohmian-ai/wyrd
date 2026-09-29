//! Unary batch bounds.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::resources::BifrostResourceError;
use num_traits::ToPrimitive;

/// Smallest accounting unit used for transport body ownership.
pub const BIFROST_TRANSPORT_QUANTUM_BYTES: usize = 64 * 1024;
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
/// Largest flattened native schema node count the IPC preflight represents.
///
/// A fixed structural parser bound, not a memory ceiling: the table's physical
/// leaf count is validated at registration.
pub const BIFROST_NATIVE_FIELD_LIMIT: usize = 256;
/// Largest native record-batch count the IPC preflight represents.
///
/// A fixed structural parser bound, not a memory ceiling.
pub const BIFROST_NATIVE_SOURCE_LIMIT: usize = 64;
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

/// Byte-weighted process admission for encoded HTTP and tonic bodies.
#[derive(Debug, Clone)]
pub struct BifrostTransportAdmission {
    /// Exact rounded live ownership shared by every transport surface.
    used: Arc<AtomicUsize>,
    /// Checked aggregate capacity derived from the process unmanaged-memory plan.
    capacity: usize,
    /// Boot-frozen maximum for one declared or decoded message.
    message_limit: usize,
}

impl BifrostTransportAdmission {
    /// Constructs transport admission under independently selected message and aggregate bounds.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::InvalidPlan`] when the selected maximum
    /// is zero or cannot fit once inside the aggregate encoded-body capacity.
    pub fn new(
        limit_bytes: usize,
        message_limit_bytes: usize,
    ) -> Result<Self, BifrostResourceError> {
        let rounded_message_limit = message_limit_bytes
            .checked_add(BIFROST_TRANSPORT_QUANTUM_BYTES - 1)
            .map(|bytes| bytes / BIFROST_TRANSPORT_QUANTUM_BYTES * BIFROST_TRANSPORT_QUANTUM_BYTES);
        if message_limit_bytes == 0
            || rounded_message_limit.is_none_or(|rounded| rounded > limit_bytes)
        {
            return Err(BifrostResourceError::InvalidPlan {
                detail: format!(
                    "transport message limit {message_limit_bytes} must fit aggregate capacity {limit_bytes}"
                ),
            });
        }
        Ok(Self {
            used: Arc::new(AtomicUsize::new(0)),
            capacity: limit_bytes,
            message_limit: message_limit_bytes,
        })
    }

    /// Builds a live admission object for tests that need one without a plan.
    ///
    /// Every serving path derives both bounds from the resource plan, so this
    /// exists only so unit tests and test shells can exercise byte-weighted
    /// admission without a resource observation. The aggregate is sized to hold
    /// exactly two default maximum ingest messages, which is what the boundary
    /// tests assert against; it is not a production budget and must not be read
    /// as one.
    ///
    /// # Panics
    ///
    /// Panics when the fixed test bounds stop satisfying [`Self::new`], which
    /// can only happen if the constants below are edited inconsistently.
    #[must_use]
    pub fn for_tests() -> Self {
        Self::new(
            2 * BIFROST_INGEST_REQUEST_LIMIT_BYTES,
            BIFROST_INGEST_REQUEST_LIMIT_BYTES,
        )
        .expect("fixed test transport bounds are internally consistent")
    }
    /// Acquires the rounded declared or frame length before body allocation.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Occupied`] when the message exceeds the
    /// boot-selected individual cap or the next aggregate ownership exceeds
    /// the independently derived process capacity.
    pub fn try_acquire(
        &self,
        declared_bytes: usize,
    ) -> Result<BifrostTransportLease, BifrostResourceError> {
        if declared_bytes > self.message_limit {
            record_transport("refused_message_limit", self.used_bytes());
            return Err(BifrostResourceError::Occupied {
                detail: format!(
                    "transport message exceeds the {} byte encoded-body limit",
                    self.message_limit
                ),
            });
        }
        let rounded = declared_bytes
            .checked_add(BIFROST_TRANSPORT_QUANTUM_BYTES - 1)
            .ok_or_else(|| BifrostResourceError::InvalidPlan {
                detail: "transport admission arithmetic overflow".to_owned(),
            })?
            / BIFROST_TRANSPORT_QUANTUM_BYTES
            * BIFROST_TRANSPORT_QUANTUM_BYTES;
        if self
            .used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(rounded)
                    .filter(|next| *next <= self.capacity)
            })
            .is_err()
        {
            record_transport("refused_occupied", self.used_bytes());
            return Err(BifrostResourceError::Occupied {
                detail: "transport encoded-body budget is occupied".to_owned(),
            });
        }
        record_transport("acquired", self.used_bytes());
        Ok(BifrostTransportLease {
            bytes: rounded,
            admission: self.clone(),
        })
    }

    /// Acquires a pessimistic maximum-body charge for unknown HTTP lengths.
    ///
    /// # Errors
    ///
    /// Returns a typed capacity refusal when the selected maximum body cannot fit.
    pub fn try_acquire_unknown(&self) -> Result<BifrostTransportLease, BifrostResourceError> {
        self.try_acquire(self.message_limit)
    }

    /// Returns exact rounded live ownership for telemetry and tests.
    #[must_use]
    pub fn used_bytes(&self) -> usize {
        self.used.load(Ordering::Acquire)
    }

    /// Returns the checked aggregate encoded-body capacity.
    #[must_use]
    pub const fn limit_bytes(&self) -> usize {
        self.capacity
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
    /// Rounded bytes charged before allocation.
    bytes: usize,
    /// Shared process admission that receives cancellation and terminal drops.
    admission: BifrostTransportAdmission,
}

impl BifrostTransportLease {
    /// Returns the rounded byte charge retained by this lease.
    #[must_use]
    pub const fn bytes(&self) -> usize {
        self.bytes
    }
}

impl Drop for BifrostTransportLease {
    /// Releases exact ownership on success, error, or cancellation.
    fn drop(&mut self) {
        let prior = self.admission.used.fetch_sub(self.bytes, Ordering::AcqRel);
        debug_assert!(
            prior >= self.bytes,
            "transport lease release must not underflow"
        );
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

    /// Weighted transport admission honors exact boundaries and cancellation release.
    ///
    /// # Panics
    ///
    /// Panics when an exact-boundary acquisition unexpectedly fails.
    #[test]
    fn bifrost_transport_admission_is_byte_weighted_and_exact_at_boundaries() {
        let admission = BifrostTransportAdmission::for_tests();
        let first = admission
            .try_acquire(BIFROST_INGEST_REQUEST_LIMIT_BYTES)
            .expect("first maximum body");
        let second = admission
            .try_acquire(BIFROST_INGEST_REQUEST_LIMIT_BYTES)
            .expect("equal aggregate boundary succeeds");
        assert!(admission.try_acquire(1).is_err());
        drop(first);
        let small = (0..8)
            .map(|_| admission.try_acquire(64 * 1024).expect("small body"))
            .collect::<Vec<_>>();
        assert_eq!(small.len(), 8);
        drop(small);
        drop(second);
        let unknown = admission.try_acquire_unknown().expect("unknown body");
        assert_eq!(unknown.bytes(), BIFROST_INGEST_REQUEST_LIMIT_BYTES);
        drop(unknown);
        assert_eq!(admission.used_bytes(), 0);
        assert!(
            admission
                .try_acquire(BIFROST_INGEST_REQUEST_LIMIT_BYTES + 1)
                .is_err()
        );
    }

    /// A selected request maximum remains distinct from aggregate capacity.
    ///
    /// # Panics
    ///
    /// Panics when a supported selected bound is rejected or exact accounting drifts.
    #[test]
    fn bifrost_transport_admission_uses_selected_message_limit_and_independent_aggregate() {
        let selected = 48 * 1024 * 1024;
        let aggregate = selected * 2;
        let admission = BifrostTransportAdmission::new(aggregate, selected)
            .expect("selected maximum fits aggregate capacity");

        let known = admission
            .try_acquire(selected)
            .expect("selected maximum succeeds");
        assert_eq!(known.bytes(), selected);
        assert!(admission.try_acquire(selected + 1).is_err());

        let unknown = admission
            .try_acquire_unknown()
            .expect("unknown length charges selected maximum");
        assert_eq!(unknown.bytes(), selected);
        assert_eq!(admission.used_bytes(), aggregate);
        assert!(
            admission.try_acquire(1).is_err(),
            "aggregate occupancy remains independent from the per-message bound"
        );
        assert_eq!(admission.limit_bytes(), aggregate);
        assert_eq!(admission.message_limit_bytes(), selected);
        drop(known);
        drop(unknown);
        assert_eq!(admission.used_bytes(), 0);
        assert!(BifrostTransportAdmission::new(selected - 1, selected).is_err());
    }
}
