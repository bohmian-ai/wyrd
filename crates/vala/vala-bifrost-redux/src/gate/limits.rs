//! Unary batch bounds.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::resources::BifrostResourceError;
use num_traits::ToPrimitive;

/// Process-wide live encoded-body budget retained inside unmanaged memory.
pub const BIFROST_TRANSPORT_LIMIT_BYTES: usize = 400 * 1024 * 1024;
/// Smallest accounting unit used for transport body ownership.
pub const BIFROST_TRANSPORT_QUANTUM_BYTES: usize = 64 * 1024;
/// Default individual encoded HTTP or tonic message selected for Bifrost.
pub const BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES: usize = 200 * 1024 * 1024;
/// Largest canonical native schema accepted by V1.
pub const BIFROST_NATIVE_FIELD_LIMIT: usize = 256;
/// Largest canonical native record-batch count accepted by V1.
pub const BIFROST_NATIVE_SOURCE_LIMIT: usize = 64;
/// Largest logical row count accepted by one V1 request.
pub const BIFROST_INGEST_ROW_LIMIT: usize = 131_072;
/// Fixed WAL header and digest scratch maximum.
pub const BIFROST_WAL_WORKSPACE_LIMIT_BYTES: usize = 4 * 1024;

/// Immutable V1 OTLP limits shared by the server adapter and Scribe planner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OtlpWireLimits {
    /// Largest encoded protobuf or JSON request accepted by an adapter.
    pub request_bytes: usize,
    /// Largest number of resource groups in one export.
    pub resources: usize,
    /// Largest number of instrumentation-scope groups in one export.
    pub scopes: usize,
    /// Largest number of signal records in one export.
    pub records: usize,
    /// Largest number of attribute entries in one export.
    pub attributes: usize,
    /// Largest cumulative key, value, body, and identifier byte count.
    pub value_bytes: usize,
    /// Largest recursive `AnyValue` nesting depth.
    pub value_depth: usize,
    /// Largest number of distinct event-day partitions.
    pub event_days: usize,
}

/// Canonical OTLP V1 defaults used to initialize decode and projection limits.
pub const OTLP_WIRE_LIMITS: OtlpWireLimits = OtlpWireLimits {
    request_bytes: BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES,
    resources: 4_096,
    scopes: 8_192,
    records: 131_072,
    attributes: 1_048_576,
    value_bytes: 32 * 1024 * 1024,
    value_depth: 8,
    event_days: 32,
};

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

impl Default for BifrostTransportAdmission {
    fn default() -> Self {
        Self::new(
            BIFROST_TRANSPORT_LIMIT_BYTES,
            BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES,
        )
        .expect("default transport message limit fits aggregate capacity")
    }
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
    /// Maximum size of one decompressed Arrow IPC batch.
    pub max_frame_bytes: usize,
    /// tonic `max_decoding_message_size` (default 4 MiB silently drops large
    /// frames; the server raises it and enforces its own cap instead).
    pub max_decoding_message_size: usize,
    /// Immutable operator-selected OTLP count and material ceilings.
    pub otlp: OtlpWireLimits,
    /// Maximum canonical native schema field count.
    pub native_fields: usize,
    /// Maximum canonical native record-batch/source count.
    pub native_sources: usize,
    /// Maximum logical rows in one native or OTLP request.
    pub rows: usize,
    /// Fixed WAL header and digest scratch retained by one ingress root.
    pub wal_workspace_bytes: usize,
}

impl Default for IngestLimits {
    fn default() -> Self {
        Self {
            max_frame_bytes: BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES,
            max_decoding_message_size: BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES + 64 * 1024,
            otlp: OTLP_WIRE_LIMITS,
            native_fields: BIFROST_NATIVE_FIELD_LIMIT,
            native_sources: BIFROST_NATIVE_SOURCE_LIMIT,
            rows: BIFROST_INGEST_ROW_LIMIT,
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
        let admission = BifrostTransportAdmission::default();
        let first = admission
            .try_acquire(BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES)
            .expect("first maximum body");
        let second = admission
            .try_acquire(BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES)
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
        assert_eq!(unknown.bytes(), BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES);
        drop(unknown);
        assert_eq!(admission.used_bytes(), 0);
        assert!(
            admission
                .try_acquire(BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES + 1)
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
        let selected = 201 * 1024 * 1024;
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
