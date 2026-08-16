//! Checked, caller-owned material buffers for Arrow normalization and IPC.
//!
//! This module computes the complete simultaneously live material peak before
//! invoking the caller's reservation authority. The resulting buffers have
//! fixed lengths, cannot grow, and retain the opaque reservation until drop or
//! an explicit ownership transfer.

use std::fmt;

/// Publicly observable facts needed to bound one Arrow normalization and IPC operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArrowIpcMaterialFacts {
    /// Input bytes retained while normalized Arrow and encoded IPC coexist.
    pub retained_input_bytes: usize,
    /// Destination Arrow value-buffer bytes.
    pub destination_values_bytes: usize,
    /// Destination Arrow offset-buffer bytes.
    pub destination_offsets_bytes: usize,
    /// Number of destination validity bits, rounded to bytes by the plan.
    pub destination_validity_bits: usize,
    /// IPC message metadata bytes before public alignment padding.
    pub ipc_metadata_bytes: usize,
    /// IPC body bytes before public alignment padding.
    pub ipc_body_bytes: usize,
    /// IPC framing bytes outside the aligned metadata and body.
    pub ipc_prefix_bytes: usize,
    /// Public Arrow IPC alignment used for metadata and body padding.
    pub ipc_alignment: usize,
}

/// A checked allocation plan for one simultaneous Arrow normalization and IPC result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArrowIpcMaterialPlan {
    /// Input bytes that remain live through construction.
    retained_input_bytes: usize,
    /// Planned fixed Arrow values length.
    destination_values_bytes: usize,
    /// Planned fixed Arrow offsets length.
    destination_offsets_bytes: usize,
    /// Planned rounded Arrow validity length.
    destination_validity_bytes: usize,
    /// Planned framed and aligned IPC output length.
    ipc_output_bytes: usize,
    /// Complete planned simultaneous material charge.
    simultaneous_peak_bytes: usize,
}

impl ArrowIpcMaterialPlan {
    /// Computes every material component with checked arithmetic and public IPC alignment.
    ///
    /// # Errors
    ///
    /// Returns [`ArrowIpcMaterialError::InvalidAlignment`] for a zero or
    /// non-power-of-two alignment. Returns [`ArrowIpcMaterialError::Overflow`]
    /// when validity rounding, alignment, or the simultaneous peak overflows.
    pub fn checked(facts: ArrowIpcMaterialFacts) -> Result<Self, ArrowIpcMaterialError<()>> {
        if !facts.ipc_alignment.is_power_of_two() {
            return Err(ArrowIpcMaterialError::InvalidAlignment {
                alignment: facts.ipc_alignment,
            });
        }
        let validity_with_rounding = facts
            .destination_validity_bits
            .checked_add(7)
            .ok_or(ArrowIpcMaterialError::Overflow)?;
        let destination_validity_bytes = validity_with_rounding / 8;
        let metadata_with_prefix = facts
            .ipc_prefix_bytes
            .checked_add(facts.ipc_metadata_bytes)
            .ok_or(ArrowIpcMaterialError::Overflow)?;
        let ipc_metadata_bytes = checked_align(metadata_with_prefix, facts.ipc_alignment)?;
        let ipc_body_bytes = checked_align(facts.ipc_body_bytes, facts.ipc_alignment)?;
        let ipc_output_bytes = ipc_metadata_bytes
            .checked_add(ipc_body_bytes)
            .ok_or(ArrowIpcMaterialError::Overflow)?;
        let simultaneous_peak_bytes = [
            facts.retained_input_bytes,
            facts.destination_values_bytes,
            facts.destination_offsets_bytes,
            destination_validity_bytes,
            ipc_output_bytes,
        ]
        .into_iter()
        .try_fold(0usize, |sum, bytes| sum.checked_add(bytes))
        .ok_or(ArrowIpcMaterialError::Overflow)?;

        Ok(Self {
            retained_input_bytes: facts.retained_input_bytes,
            destination_values_bytes: facts.destination_values_bytes,
            destination_offsets_bytes: facts.destination_offsets_bytes,
            destination_validity_bytes,
            ipc_output_bytes,
            simultaneous_peak_bytes,
        })
    }

    /// Returns the complete peak that the caller must reserve before allocation.
    #[must_use]
    pub const fn simultaneous_peak_bytes(self) -> usize {
        self.simultaneous_peak_bytes
    }

    /// Returns the fixed IPC output length including framing and padding.
    #[must_use]
    pub const fn ipc_output_bytes(self) -> usize {
        self.ipc_output_bytes
    }
}

/// Materialized fixed buffer capacities retained for reconciliation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArrowIpcMaterializedCapacity {
    /// Retained input charged to the opaque caller reservation.
    pub retained_input_bytes: usize,
    /// Fixed destination values capacity.
    pub destination_values_bytes: usize,
    /// Fixed destination offsets capacity.
    pub destination_offsets_bytes: usize,
    /// Fixed destination validity capacity.
    pub destination_validity_bytes: usize,
    /// Fixed IPC output capacity.
    pub ipc_output_bytes: usize,
    /// Sum of all simultaneously retained material capacity.
    pub simultaneous_peak_bytes: usize,
}

/// Fixed Arrow normalization and IPC storage retaining one opaque caller reservation.
pub struct BoundedArrowIpc<G> {
    /// Checked facts and complete admission charge.
    plan: ArrowIpcMaterialPlan,
    /// Opaque caller authority retained solely for its ownership lifetime.
    _reservation: G,
    /// Fixed destination values storage.
    values: Box<[u8]>,
    /// Fixed destination offsets storage.
    offsets: Box<[u8]>,
    /// Fixed destination validity storage.
    validity: Box<[u8]>,
    /// Fixed framed IPC output storage.
    ipc: Box<[u8]>,
}

impl<G> fmt::Debug for BoundedArrowIpc<G> {
    /// Formats only material capacities; the opaque reservation is never exposed.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BoundedArrowIpc")
            .field("materialized", &self.materialized_capacity())
            .finish_non_exhaustive()
    }
}

impl<G> BoundedArrowIpc<G> {
    /// Checks the complete peak, reserves it from the caller, then creates fixed buffers.
    ///
    /// The reservation closure is called exactly once and before any destination
    /// material allocation. Its opaque result remains owned by this value until
    /// the value is dropped. Moving this owner transfers its buffers and guard
    /// together; the API never permits detaching the reservation.
    ///
    /// # Errors
    ///
    /// Returns checked planning errors before calling `reserve`. Returns
    /// [`ArrowIpcMaterialError::Reservation`] when the caller refuses the full
    /// peak, before allocating destination buffers.
    pub fn try_new<E>(
        facts: ArrowIpcMaterialFacts,
        reserve: impl FnOnce(usize) -> Result<G, E>,
    ) -> Result<Self, ArrowIpcMaterialError<E>> {
        let plan = ArrowIpcMaterialPlan::checked(facts).map_err(|error| match error {
            ArrowIpcMaterialError::InvalidAlignment { alignment } => {
                ArrowIpcMaterialError::InvalidAlignment { alignment }
            }
            ArrowIpcMaterialError::Overflow => ArrowIpcMaterialError::Overflow,
            ArrowIpcMaterialError::Reservation(()) => unreachable!("planning never reserves"),
        })?;
        let reservation =
            reserve(plan.simultaneous_peak_bytes).map_err(ArrowIpcMaterialError::Reservation)?;
        Ok(Self {
            plan,
            _reservation: reservation,
            values: vec![0; plan.destination_values_bytes].into_boxed_slice(),
            offsets: vec![0; plan.destination_offsets_bytes].into_boxed_slice(),
            validity: vec![0; plan.destination_validity_bytes].into_boxed_slice(),
            ipc: vec![0; plan.ipc_output_bytes].into_boxed_slice(),
        })
    }

    /// Returns the checked admission plan retained by this result.
    #[must_use]
    pub const fn plan(&self) -> ArrowIpcMaterialPlan {
        self.plan
    }

    /// Returns the fixed normalization value buffer.
    #[must_use]
    pub fn values_mut(&mut self) -> &mut [u8] {
        &mut self.values
    }

    /// Returns the fixed normalization offset buffer.
    #[must_use]
    pub fn offsets_mut(&mut self) -> &mut [u8] {
        &mut self.offsets
    }

    /// Returns the fixed normalization validity buffer.
    #[must_use]
    pub fn validity_mut(&mut self) -> &mut [u8] {
        &mut self.validity
    }

    /// Returns the fixed IPC output buffer.
    #[must_use]
    pub fn ipc_mut(&mut self) -> &mut [u8] {
        &mut self.ipc
    }

    /// Observes every materialized capacity without exposing the reservation.
    #[must_use]
    pub fn materialized_capacity(&self) -> ArrowIpcMaterializedCapacity {
        ArrowIpcMaterializedCapacity {
            retained_input_bytes: self.plan.retained_input_bytes,
            destination_values_bytes: self.values.len(),
            destination_offsets_bytes: self.offsets.len(),
            destination_validity_bytes: self.validity.len(),
            ipc_output_bytes: self.ipc.len(),
            simultaneous_peak_bytes: self.plan.simultaneous_peak_bytes,
        }
    }
}

/// Checked planning or caller-reservation refusal for bounded Arrow material.
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum ArrowIpcMaterialError<E> {
    /// IPC alignment must be a nonzero power of two.
    #[error("Arrow IPC alignment {alignment} is not a nonzero power of two")]
    InvalidAlignment {
        /// Rejected public IPC alignment.
        alignment: usize,
    },
    /// A material component, padding calculation, or simultaneous sum overflowed.
    #[error("Arrow IPC material bound overflowed")]
    Overflow,
    /// The caller's authoritative budget refused the complete peak.
    #[error("Arrow IPC material reservation refused: {0}")]
    Reservation(E),
}

/// Aligns `bytes` upward with checked public power-of-two arithmetic.
///
/// # Errors
///
/// Returns [`ArrowIpcMaterialError::Overflow`] when padding overflows.
fn checked_align(bytes: usize, alignment: usize) -> Result<usize, ArrowIpcMaterialError<()>> {
    let mask = alignment - 1;
    bytes
        .checked_add(mask)
        .map(|value| value & !mask)
        .ok_or(ArrowIpcMaterialError::Overflow)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{ArrowIpcMaterialError, ArrowIpcMaterialFacts, BoundedArrowIpc};

    /// Returns representative public Arrow and IPC facts with alignment padding.
    fn facts() -> ArrowIpcMaterialFacts {
        ArrowIpcMaterialFacts {
            retained_input_bytes: 100,
            destination_values_bytes: 80,
            destination_offsets_bytes: 20,
            destination_validity_bits: 9,
            ipc_metadata_bytes: 17,
            ipc_body_bytes: 65,
            ipc_prefix_bytes: 8,
            ipc_alignment: 8,
        }
    }

    /// Checked formula includes retained input, Arrow buffers, IPC framing, and padding.
    #[test]
    fn bounded_arrow_checked_complete_simultaneous_peak() {
        let bounded = BoundedArrowIpc::try_new(facts(), |_| Ok::<_, ()>(())).expect("admitted");
        let capacity = bounded.materialized_capacity();
        assert_eq!(capacity.destination_validity_bytes, 2);
        assert_eq!(capacity.ipc_output_bytes, 104);
        assert_eq!(capacity.simultaneous_peak_bytes, 306);
    }

    /// Overflow refuses before consulting the reservation authority or allocating.
    #[test]
    fn bounded_arrow_overflow_refuses_before_reservation() {
        let calls = AtomicUsize::new(0);
        let mut overflowing = facts();
        overflowing.destination_values_bytes = usize::MAX;
        let result = BoundedArrowIpc::try_new(overflowing, |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok::<_, ()>(())
        });
        assert!(matches!(result, Err(ArrowIpcMaterialError::Overflow)));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    /// Budget refusal occurs before any result exists and preserves the authority's state.
    #[test]
    fn bounded_arrow_reservation_refusal_precedes_allocation() {
        let error = BoundedArrowIpc::try_new(facts(), |_| Err::<(), _>("full"));
        assert!(matches!(
            error,
            Err(ArrowIpcMaterialError::Reservation("full"))
        ));
    }

    /// Fixed boxed slices expose no operation that can grow admitted material.
    #[test]
    fn bounded_arrow_material_has_fixed_lengths() {
        let mut bounded = BoundedArrowIpc::try_new(facts(), |_| Ok::<_, ()>(())).expect("admitted");
        assert_eq!(bounded.values_mut().len(), 80);
        assert_eq!(bounded.offsets_mut().len(), 20);
        assert_eq!(bounded.validity_mut().len(), 2);
        assert_eq!(bounded.ipc_mut().len(), 104);
    }

    /// Ownership transfer retains the same guard until the transferred result drops.
    #[test]
    fn bounded_arrow_transfer_releases_exactly_once() {
        struct Guard(Arc<AtomicUsize>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let releases = Arc::new(AtomicUsize::new(0));
        let bounded =
            BoundedArrowIpc::try_new(facts(), |_| Ok::<_, ()>(Guard(Arc::clone(&releases))))
                .expect("admitted");
        let transferred = transfer(bounded);
        assert_eq!(releases.load(Ordering::SeqCst), 0);
        drop(transferred);
        assert_eq!(releases.load(Ordering::SeqCst), 1);
    }

    /// Moves a bounded owner across a representative ownership boundary.
    fn transfer<G>(owned: BoundedArrowIpc<G>) -> BoundedArrowIpc<G> {
        owned
    }

    /// Cancellation-shaped drop and panic unwinding both release the opaque guard once.
    #[test]
    fn bounded_arrow_cancellation_and_panic_release() {
        struct Guard(Arc<AtomicUsize>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let releases = Arc::new(AtomicUsize::new(0));
        let cancelled =
            BoundedArrowIpc::try_new(facts(), |_| Ok::<_, ()>(Guard(Arc::clone(&releases))))
                .expect("admitted");
        drop(cancelled);
        assert_eq!(releases.load(Ordering::SeqCst), 1);

        let unwind_releases = Arc::clone(&releases);
        let unwind = std::panic::catch_unwind(move || {
            let _owned = BoundedArrowIpc::try_new(facts(), |_| {
                Ok::<_, ()>(Guard(Arc::clone(&unwind_releases)))
            })
            .expect("admitted");
            panic!("exercise terminal unwind");
        });
        assert!(unwind.is_err());
        assert_eq!(releases.load(Ordering::SeqCst), 2);
    }
}
