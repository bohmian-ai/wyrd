//! Per-tenant, per-table contention accounting for Scribe.
//!
//! Pod-global counters alone cannot keep one hot table from starving its
//! neighbours: a tenant that saturates the global in-flight ceiling makes every
//! other tenant's append fail with the same busy error, and nothing in a global
//! counter says whose fault that is or whose work should have been protected.
//!
//! This module adds the missing dimension. Each active table owns one
//! [`crate::scribe::geometry::ContentionReserveVector`] — a protected cell in
//! every governed category, installed as a whole at activation and never
//! borrowable by anyone else however idle it looks. Capacity beyond the
//! guaranteed vectors is elastic surplus: any active table may draw on it, and
//! a table that has exhausted both its reserve and its share of the surplus is
//! refused by name rather than pushing the refusal onto a neighbour.
//!
//! The ledger owns the accounting only. It does not perform IO, does not know
//! what a batch is, and does not decide when a table stops being active; those
//! belong to the admission and shard owners that hold it.

use std::collections::HashMap;
use std::sync::Mutex;

use wyrd_spec::ids::DataTenantId;

use crate::catalog::TableRef;
use crate::contracts::ScribeError;
use crate::scribe::geometry::{
    ContentionCategory, ContentionReserveVector, ScribeArtifactPolicy, ScribeGlobalCapacity,
};

/// Identity of one contention cell.
///
/// A cell is per tenant *and* per table because both separations matter: two
/// tenants writing the same logical table name must not share a cell, and one
/// tenant's hot table must not be able to exclude its own sibling system table.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentionKey {
    /// Owning tenant.
    pub tenant: DataTenantId,
    /// Owning logical table.
    pub table: TableRef,
}

impl ContentionKey {
    /// Builds one cell identity.
    #[must_use]
    pub const fn new(tenant: DataTenantId, table: TableRef) -> Self {
        Self { tenant, table }
    }
}

/// Live usage of one active table across every governed category.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct CellUsage {
    /// Committed amount per category, indexed by `ContentionCategory::ALL`.
    per_category: [usize; ContentionCategory::ALL.len()],
}

impl CellUsage {
    /// Returns this cell's committed amount in one category.
    const fn get(&self, category: ContentionCategory) -> usize {
        self.per_category[category as usize]
    }

    /// Sets this cell's committed amount in one category.
    const fn set(&mut self, category: ContentionCategory, amount: usize) {
        self.per_category[category as usize] = amount;
    }
}

/// Mutable ledger state guarded by one lock.
///
/// Cells and totals move together under the same lock so a concurrent
/// activation can never observe a total that counts a vector the map does not
/// yet contain, which would let the pod over-commit by exactly one vector.
#[derive(Debug, Default)]
struct LedgerState {
    /// Installed cells by identity.
    cells: HashMap<ContentionKey, CellUsage>,
    /// Pod-wide committed amount per category across every cell.
    committed: [usize; ContentionCategory::ALL.len()],
}

impl LedgerState {
    /// Returns the pod-wide committed amount in one category.
    const fn committed(&self, category: ContentionCategory) -> usize {
        self.committed[category as usize]
    }

    /// Sets the pod-wide committed amount in one category.
    const fn set_committed(&mut self, category: ContentionCategory, amount: usize) {
        self.committed[category as usize] = amount;
    }
}

/// Why one contention charge or activation was refused.
///
/// The variants distinguish the two refusals a caller must handle differently:
/// a table that has exceeded its own protected reserve and the shared surplus
/// is busy and may retry, while a pod that cannot install another cell at all
/// is at its activation width and retrying the same table will not help.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContentionRefusal {
    /// The pod cannot hold another complete reserve vector.
    #[error(
        "Scribe cannot activate another table: `{category}` holds {capacity} and {active} active tables already reserve {committed}"
    )]
    ActivationWidth {
        /// Fixed-cardinality category label that ran out first.
        category: &'static str,
        /// Pod capacity in that category.
        capacity: usize,
        /// Already-committed total in that category.
        committed: usize,
        /// Number of tables already active.
        active: usize,
    },
    /// One table exhausted both its protected reserve and the shared surplus.
    #[error(
        "Scribe table contention limit reached for `{category}`: reserve {reserve} and available surplus cannot cover {requested}"
    )]
    Exhausted {
        /// Fixed-cardinality category label that ran out.
        category: &'static str,
        /// This table's protected reserve in that category.
        reserve: usize,
        /// Amount the caller asked for.
        requested: usize,
    },
    /// A charge referenced a table that holds no installed cell.
    #[error("Scribe contention charge for an inactive table")]
    Inactive,
    /// The ledger lock was poisoned by a panicking holder.
    #[error("Scribe contention ledger lock poisoned: {detail}")]
    Poisoned {
        /// Lock failure detail.
        detail: String,
    },
}

impl From<ContentionRefusal> for ScribeError {
    /// Maps a contention refusal onto the Scribe error a caller already handles.
    ///
    /// Exhaustion and activation width are backpressure, so they surface as
    /// `IngestBusy` naming the table that could not proceed. The remaining
    /// variants are accounting invariants, not caller-visible pressure, so they
    /// surface as `Internal`.
    fn from(refusal: ContentionRefusal) -> Self {
        match refusal {
            ContentionRefusal::ActivationWidth { .. } | ContentionRefusal::Exhausted { .. } => {
                Self::IngestBusy {
                    table: refusal.to_string(),
                }
            }
            ContentionRefusal::Inactive | ContentionRefusal::Poisoned { .. } => Self::Internal {
                detail: refusal.to_string(),
            },
        }
    }
}

/// Pod-wide per-table contention ledger.
///
/// Constructed once from the validated [`ScribeArtifactPolicy`] and the pod
/// capacity that startup already proved can hold the guaranteed vectors, so the
/// ledger never has to re-derive either.
#[derive(Debug)]
pub struct ScribeContentionLedger {
    /// Policy the reserve vector and required totals come from.
    policy: ScribeArtifactPolicy,
    /// Pod capacity each category is checked against.
    capacity: ScribeGlobalCapacity,
    /// Installed cells and their committed totals.
    state: Mutex<LedgerState>,
}

impl ScribeContentionLedger {
    /// Builds a ledger over one validated policy and the pod's real capacity.
    #[must_use]
    pub fn new(policy: ScribeArtifactPolicy, capacity: ScribeGlobalCapacity) -> Self {
        Self {
            policy,
            capacity,
            state: Mutex::new(LedgerState::default()),
        }
    }

    /// Returns the reserve vector every active table installs.
    #[must_use]
    pub const fn reserve_vector(&self) -> ContentionReserveVector {
        self.policy.reserve_vector()
    }

    /// Installs one table's complete reserve vector, making it active.
    ///
    /// Activation is all-or-nothing across every category. A table that
    /// activated with only some of its reserve could acknowledge an append it
    /// has no staging, merge, or upload capacity to finish, which is exactly the
    /// partial-installation failure the whole-vector rule exists to prevent.
    /// Activating an already-active table is idempotent and reserves nothing
    /// further.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::ActivationWidth`] naming the first category
    /// whose capacity cannot hold one more vector, and
    /// [`ContentionRefusal::Poisoned`] when the ledger lock is poisoned.
    pub fn activate(&self, key: &ContentionKey) -> Result<(), ContentionRefusal> {
        let vector = self.reserve_vector();
        let mut state = self.lock()?;
        if state.cells.contains_key(key) {
            return Ok(());
        }
        for category in ContentionCategory::ALL {
            let reserve = vector.component(category);
            let committed = state.committed(category);
            let capacity = self.capacity.component(category);
            if committed
                .checked_add(reserve)
                .is_none_or(|next| next > capacity)
            {
                return Err(ContentionRefusal::ActivationWidth {
                    category: category.label(),
                    capacity,
                    committed,
                    active: state.cells.len(),
                });
            }
        }
        for category in ContentionCategory::ALL {
            let next = state.committed(category) + vector.component(category);
            state.set_committed(category, next);
        }
        state.cells.insert(key.clone(), CellUsage::default());
        Ok(())
    }

    /// Releases one table's cell and every category it still held.
    ///
    /// Deactivating a table that is not active is a no-op rather than an error:
    /// the shard and admission owners may both reach the same conclusion about
    /// an idle table, and neither should fail because the other got there first.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::Poisoned`] when the ledger lock is poisoned.
    pub fn deactivate(&self, key: &ContentionKey) -> Result<(), ContentionRefusal> {
        let vector = self.reserve_vector();
        let mut state = self.lock()?;
        let Some(usage) = state.cells.remove(key) else {
            return Ok(());
        };
        for category in ContentionCategory::ALL {
            // Committed holds the reserve plus whatever surplus this cell drew
            // beyond it, so both come back at once.
            let released = vector
                .component(category)
                .max(usage.get(category))
                .min(state.committed(category));
            let next = state.committed(category) - released;
            state.set_committed(category, next);
        }
        Ok(())
    }

    /// Charges `amount` in one category to an active table.
    ///
    /// A charge inside the table's protected reserve always succeeds and costs
    /// the pod nothing new, because that reserve was already committed at
    /// activation. A charge beyond the reserve draws on the shared surplus and
    /// succeeds only while unreserved capacity remains, so a hot table can grow
    /// into idle capacity without ever being able to take a neighbour's cell.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::Inactive`] when the table holds no cell,
    /// [`ContentionRefusal::Exhausted`] when reserve and surplus together cannot
    /// cover the request, and [`ContentionRefusal::Poisoned`] when the ledger
    /// lock is poisoned.
    pub fn charge(
        &self,
        key: &ContentionKey,
        category: ContentionCategory,
        amount: usize,
    ) -> Result<(), ContentionRefusal> {
        let reserve = self.reserve_vector().component(category);
        let capacity = self.capacity.component(category);
        let mut state = self.lock()?;
        let usage = state
            .cells
            .get(key)
            .copied()
            .ok_or(ContentionRefusal::Inactive)?;
        let current = usage.get(category);
        let Some(next_usage) = current.checked_add(amount) else {
            return Err(ContentionRefusal::Exhausted {
                category: category.label(),
                reserve,
                requested: amount,
            });
        };
        if next_usage > reserve {
            // Only the part above the reserve consumes anything shared; the
            // reserve itself was committed once, at activation.
            let extra = next_usage - reserve.max(current);
            let committed = state.committed(category);
            if committed
                .checked_add(extra)
                .is_none_or(|next| next > capacity)
            {
                return Err(ContentionRefusal::Exhausted {
                    category: category.label(),
                    reserve,
                    requested: amount,
                });
            }
            state.set_committed(category, committed + extra);
        }
        if let Some(cell) = state.cells.get_mut(key) {
            cell.set(category, next_usage);
        }
        Ok(())
    }

    /// Returns `amount` in one category to an active table's cell.
    ///
    /// Releasing more than the cell holds is an accounting bug rather than a
    /// caller error, so it saturates at zero instead of wrapping into an
    /// enormous usage that would lock the table out permanently.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::Inactive`] when the table holds no cell, and
    /// [`ContentionRefusal::Poisoned`] when the ledger lock is poisoned.
    pub fn release(
        &self,
        key: &ContentionKey,
        category: ContentionCategory,
        amount: usize,
    ) -> Result<(), ContentionRefusal> {
        let reserve = self.reserve_vector().component(category);
        let mut state = self.lock()?;
        let usage = state
            .cells
            .get(key)
            .copied()
            .ok_or(ContentionRefusal::Inactive)?;
        let current = usage.get(category);
        let next_usage = current.saturating_sub(amount);
        // Mirror `charge`: only usage above the reserve ever moved the shared
        // total, so only that part comes back.
        let above_before = current.saturating_sub(reserve);
        let above_after = next_usage.saturating_sub(reserve);
        let returned = (above_before - above_after).min(state.committed(category));
        let committed = state.committed(category) - returned;
        state.set_committed(category, committed);
        if let Some(cell) = state.cells.get_mut(key) {
            cell.set(category, next_usage);
        }
        Ok(())
    }

    /// Returns the number of currently active cells.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::Poisoned`] when the ledger lock is poisoned.
    pub fn active_cells(&self) -> Result<usize, ContentionRefusal> {
        Ok(self.lock()?.cells.len())
    }

    /// Returns one table's committed amount in one category.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::Inactive`] when the table holds no cell, and
    /// [`ContentionRefusal::Poisoned`] when the ledger lock is poisoned.
    pub fn usage(
        &self,
        key: &ContentionKey,
        category: ContentionCategory,
    ) -> Result<usize, ContentionRefusal> {
        let state = self.lock()?;
        state
            .cells
            .get(key)
            .map(|usage| usage.get(category))
            .ok_or(ContentionRefusal::Inactive)
    }

    /// Returns the pod-wide committed total in one category.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::Poisoned`] when the ledger lock is poisoned.
    pub fn committed(&self, category: ContentionCategory) -> Result<usize, ContentionRefusal> {
        Ok(self.lock()?.committed(category))
    }

    /// Locks the ledger state, converting a poisoned lock into a typed refusal.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::Poisoned`] when a previous holder panicked.
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, LedgerState>, ContentionRefusal> {
        self.state
            .lock()
            .map_err(|error| ContentionRefusal::Poisoned {
                detail: error.to_string(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespaces::BifrostNamespace;
    use crate::scribe::geometry::ScribeGeometry;

    /// Builds a cell identity for one tenant ordinal and table name.
    ///
    /// # Panics
    ///
    /// Panics when the synthetic tenant identifier is not accepted, which would
    /// be a fixture bug rather than a ledger behavior.
    fn key(tenant: u8, table: &str) -> ContentionKey {
        let mut bytes = [tenant; 16];
        // Force the UUIDv7 version and variant nibbles the identifier requires.
        bytes[6] = 0x70 | (bytes[6] & 0x0f);
        bytes[8] = 0x80 | (bytes[8] & 0x3f);
        ContentionKey::new(
            DataTenantId::new(uuid::Uuid::from_bytes(bytes)).expect("UUIDv7 test tenant"),
            TableRef::new(BifrostNamespace::Datasets, table),
        )
    }

    /// Builds a ledger whose capacity holds exactly `width` reserve vectors.
    ///
    /// # Panics
    ///
    /// Panics when the default policy has no representable minimum capacity.
    fn ledger_for(width: usize) -> ScribeContentionLedger {
        let policy = ScribeArtifactPolicy::new(ScribeGeometry::default());
        let vector = policy.reserve_vector();
        let capacity = ScribeGlobalCapacity {
            admission_items: vector.admission_items * width,
            admission_bytes: vector.admission_bytes * width,
            active_bytes: vector.active_bytes * width,
            immutable_bytes: vector.immutable_bytes * width,
            durable_stage_bytes: vector.durable_stage_bytes * width,
            merge_scratch_bytes: vector.merge_scratch_bytes * width,
            staging_claim_items: vector.staging_claim_items * width,
            upload_claim_items: vector.upload_claim_items * width,
        };
        ScribeContentionLedger::new(policy, capacity)
    }

    /// A table's protected cell survives a neighbour saturating the pod.
    ///
    /// # Panics
    ///
    /// Panics when a guaranteed table is refused after a neighbour has consumed
    /// every byte of surplus, which is the starvation this ledger exists to
    /// prevent.
    #[test]
    fn guaranteed_table_reserve_is_never_borrowed_by_a_hot_neighbour() {
        let ledger = ledger_for(8);
        let hot = key(1, "hot");
        let quiet = key(2, "quiet");
        ledger.activate(&hot).expect("first table activates");
        ledger.activate(&quiet).expect("second table activates");

        let reserve = ledger.reserve_vector().active_bytes;
        // The hot table takes its own reserve and then every byte of surplus
        // the six unallocated vectors represent.
        ledger
            .charge(&hot, ContentionCategory::Active, reserve)
            .expect("a table always gets its own reserve");
        ledger
            .charge(&hot, ContentionCategory::Active, reserve * 6)
            .expect("surplus is elastic and may all go to one table");
        assert!(
            ledger.charge(&hot, ContentionCategory::Active, 1).is_err(),
            "surplus is finite; the next byte would come out of a protected cell"
        );

        // The quiet table's cell was never borrowable, so its own reserve is
        // still there in full.
        ledger
            .charge(&quiet, ContentionCategory::Active, reserve)
            .expect("a protected cell is untouched by a saturated neighbour");
    }

    /// The pod refuses to activate more tables than its capacity can hold.
    ///
    /// # Panics
    ///
    /// Panics when activation over-commits, when the refusal does not name the
    /// exhausted category and the active count, or when releasing a table does
    /// not free its cell for the next one.
    #[test]
    fn activation_is_bounded_and_returns_its_whole_vector_on_release() {
        let ledger = ledger_for(2);
        ledger.activate(&key(1, "a")).expect("first activates");
        ledger.activate(&key(2, "b")).expect("second activates");
        let refusal = ledger
            .activate(&key(3, "c"))
            .expect_err("a third table exceeds the pod's activation width");
        let ContentionRefusal::ActivationWidth {
            category, active, ..
        } = refusal
        else {
            panic!("an over-wide activation must report the activation diagnostic");
        };
        assert_eq!(active, 2);
        assert_eq!(category, ContentionCategory::AdmissionItems.label());

        // Activating an already-active table reserves nothing further.
        ledger
            .activate(&key(1, "a"))
            .expect("re-activation is idempotent");
        assert_eq!(ledger.active_cells().expect("locked"), 2);

        ledger.deactivate(&key(1, "a")).expect("release succeeds");
        ledger
            .activate(&key(3, "c"))
            .expect("a released cell is available to the next table");
    }

    /// Surplus a table drew beyond its reserve returns when the table releases.
    ///
    /// # Panics
    ///
    /// Panics when releasing does not return the shared surplus, which would
    /// leak pod capacity one charge at a time.
    #[test]
    fn surplus_drawn_beyond_the_reserve_is_returned_on_release() {
        let ledger = ledger_for(4);
        let table = key(1, "a");
        ledger.activate(&table).expect("activates");
        let reserve = ledger.reserve_vector().active_bytes;
        let baseline = ledger
            .committed(ContentionCategory::Active)
            .expect("locked");

        ledger
            .charge(&table, ContentionCategory::Active, reserve)
            .expect("the reserve itself costs nothing new");
        assert_eq!(
            ledger
                .committed(ContentionCategory::Active)
                .expect("locked"),
            baseline,
            "charging inside the reserve must not move the shared total"
        );

        ledger
            .charge(&table, ContentionCategory::Active, reserve)
            .expect("surplus covers a second reserve worth");
        assert_eq!(
            ledger
                .committed(ContentionCategory::Active)
                .expect("locked"),
            baseline + reserve
        );

        ledger
            .release(&table, ContentionCategory::Active, reserve * 2)
            .expect("release succeeds");
        assert_eq!(
            ledger
                .committed(ContentionCategory::Active)
                .expect("locked"),
            baseline,
            "every surplus byte comes back"
        );
        assert_eq!(
            ledger
                .usage(&table, ContentionCategory::Active)
                .expect("still active"),
            0
        );
    }

    /// Charging an inactive table is refused rather than silently accounted.
    ///
    /// # Panics
    ///
    /// Panics when a charge against a table with no installed cell is accepted.
    #[test]
    fn charging_an_inactive_table_is_refused() {
        let ledger = ledger_for(2);
        assert_eq!(
            ledger.charge(&key(1, "a"), ContentionCategory::Active, 1),
            Err(ContentionRefusal::Inactive)
        );
        assert_eq!(
            ledger.usage(&key(1, "a"), ContentionCategory::Active),
            Err(ContentionRefusal::Inactive)
        );
    }
}
