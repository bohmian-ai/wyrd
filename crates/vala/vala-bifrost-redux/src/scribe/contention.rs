//! Per-tenant, per-table contention accounting for Scribe.
//!
//! Pod-global counters alone cannot keep one hot table from starving its
//! neighbours: a tenant that saturates the global in-flight ceiling makes every
//! other tenant's append fail with the same busy error, and nothing in a global
//! counter says whose fault that is or whose work should have been protected.
//!
//! This module adds the two missing dimensions, in the order they matter. The
//! pod-wide [`ScribeContentionLedger`] is keyed by tenant; each tenant owns one
//! [`TenantScribeLedger`] holding a whole table-width reservation and the nested
//! canonical-table [`TenantTableLedger`] cells drawn from it. A tenant that
//! activates therefore takes its configured table width out of pod capacity
//! atomically, before any of its tables arrive, so the tenant that starts
//! writing first cannot consume the vectors a later guaranteed tenant is owed.
//!
//! Three quantities govern every decision, and keeping them apart is the whole
//! point of the module:
//!
//! - The **startup reservation** is `guaranteed_width` complete
//!   [`crate::scribe::geometry::ContentionReserveVector`]s. Startup already
//!   proved the pod holds it, and it is held for the pod's whole life whether or
//!   not the guaranteed owners have arrived. It is never borrowable.
//! - A **protected cell** is one owner's slice of that reservation: a tenant's
//!   `reserved_vectors`, and inside it one vector per active table.
//! - **Surplus** is whatever capacity remains beyond the reservation. Only this
//!   is elastic, and it is not first-come: each active tenant may borrow up to
//!   an equal share of it, and each table inside a tenant up to an equal share
//!   of its tenant's share. Shares are computed independently per category, so a
//!   table that has saturated the surplus in one category still competes
//!   normally in the others.
//!
//! Shares shrink as owners arrive, and shrinking never revokes what an owner
//! already holds: an over-share owner keeps its bytes and drains them, and is
//! only refused *new* borrowed ownership. That is the difference between
//! backpressure and data loss.
//!
//! Pod-wide totals are derived from the tenant map on every read rather than
//! cached beside it. A cached total is one more thing that can drift from the
//! cells it is supposed to summarize, and the map is bounded by the configured
//! activation width, so summing it is cheap.
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

/// Live usage of one active canonical table across every governed category.
///
/// This is the innermost ledger in the hierarchy. It holds only committed
/// amounts; the protected reserve it draws from belongs to the enclosing
/// [`TenantScribeLedger`], and the ceiling it may borrow to is recomputed from
/// the live owner counts on every charge rather than cached, because a cached
/// ceiling is exactly what would let an owner keep a share a newly arrived
/// neighbour is entitled to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct TenantTableLedger {
    /// Committed amount per category, indexed by `ContentionCategory::ALL`.
    per_category: [usize; ContentionCategory::ALL.len()],
}

impl TenantTableLedger {
    /// Returns this table's committed amount in one category.
    const fn get(&self, category: ContentionCategory) -> usize {
        self.per_category[category as usize]
    }

    /// Sets this table's committed amount in one category.
    const fn set(&mut self, category: ContentionCategory, amount: usize) {
        self.per_category[category as usize] = amount;
    }
}

/// One tenant's protected cell and the canonical tables nested inside it.
///
/// `reserved_vectors` is the tenant's own width, not a count of its live tables.
/// It is taken when the tenant activates and stays taken while the tenant is
/// active, so an idle table slot inside a guaranteed tenant is still protected
/// from a hot table in another tenant. It grows past the configured table width
/// only when a complete extra vector fits pod capacity, and shrinks back toward
/// that width as tables release.
#[derive(Debug, Default)]
struct TenantScribeLedger {
    /// Nested canonical-table ledgers by table identity.
    tables: HashMap<TableRef, TenantTableLedger>,
    /// Complete reserve vectors this tenant holds out of pod capacity.
    reserved_vectors: usize,
}

impl TenantScribeLedger {
    /// Returns this tenant's total committed amount in one category.
    ///
    /// Summed across the nested table ledgers rather than tracked separately so
    /// a tenant total can never drift from the tables it summarizes.
    fn committed(&self, category: ContentionCategory) -> usize {
        self.tables
            .values()
            .map(|table| table.get(category))
            .sum::<usize>()
    }

    /// Returns this tenant's protected amount in one category.
    ///
    /// This is the floor its tables draw from before any of them touches shared
    /// surplus, and the amount no other tenant may borrow however idle it looks.
    fn protected(&self, vector: &ContentionReserveVector, category: ContentionCategory) -> usize {
        self.reserved_vectors
            .saturating_mul(vector.component(category))
    }

    /// Returns the amount this tenant currently holds beyond its protected cell.
    ///
    /// Only this part ever competes for shared surplus; the protected part was
    /// committed once, when the tenant activated.
    fn borrowed(&self, vector: &ContentionReserveVector, category: ContentionCategory) -> usize {
        self.committed(category)
            .saturating_sub(self.protected(vector, category))
    }
}

/// Mutable ledger state guarded by one lock.
///
/// Tenants and their nested tables move together under the same lock so a
/// concurrent activation can never observe a reservation the tenant map does not
/// yet contain, which would let the pod over-commit by exactly one cell.
#[derive(Debug, Default)]
struct LedgerState {
    /// Installed tenant cells by identity.
    tenants: HashMap<DataTenantId, TenantScribeLedger>,
}

impl LedgerState {
    /// Returns the number of active tenants sharing every category's surplus.
    fn active_tenants(&self) -> usize {
        self.tenants.len()
    }

    /// Returns the total complete vectors every active tenant has reserved.
    fn reserved_vectors(&self) -> usize {
        self.tenants
            .values()
            .map(|tenant| tenant.reserved_vectors)
            .sum()
    }

    /// Returns the total borrowed beyond every tenant's protected cell.
    fn borrowed(&self, vector: &ContentionReserveVector, category: ContentionCategory) -> usize {
        self.tenants
            .values()
            .map(|tenant| tenant.borrowed(vector, category))
            .sum()
    }
}

/// Why one contention charge or activation was refused.
///
/// The variants distinguish the refusals a caller must handle differently: an
/// owner that has exceeded its protected reserve and its soft share of the
/// surplus is busy and may retry, while a pod that cannot install another cell
/// at all is at its activation width and retrying will not help.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContentionRefusal {
    /// The pod cannot hold another complete tenant or table reservation.
    #[error(
        "Scribe cannot activate another {scope}: `{category}` holds {capacity} and {active} active tenants already reserve {committed}"
    )]
    ActivationWidth {
        /// Which level of the hierarchy could not be widened.
        scope: &'static str,
        /// Fixed-cardinality category label that ran out first.
        category: &'static str,
        /// Pod capacity in that category.
        capacity: usize,
        /// Already-committed total in that category.
        committed: usize,
        /// Number of tenants already active.
        active: usize,
    },
    /// One owner exhausted both its protected reserve and its soft share.
    #[error(
        "Scribe {scope} contention limit reached for `{category}`: protected {protected} and soft share ceiling {ceiling} cannot cover {requested}"
    )]
    Exhausted {
        /// Whether the tenant or the table ran out first.
        scope: &'static str,
        /// Fixed-cardinality category label that ran out.
        category: &'static str,
        /// The owner's protected amount in that category.
        protected: usize,
        /// The owner's protected amount plus its equal share of the surplus.
        ceiling: usize,
        /// Amount the caller asked for.
        requested: usize,
    },
    /// A charge referenced a tenant or table that holds no installed cell.
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
    /// `IngestBusy` naming the owner that could not proceed. The remaining
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

/// Pod-wide tenant-keyed contention ledger.
///
/// Constructed once from the validated [`ScribeArtifactPolicy`] and the pod
/// capacity that startup already proved can hold the guaranteed vectors, so the
/// ledger never has to re-derive or re-validate either.
#[derive(Debug)]
pub struct ScribeContentionLedger {
    /// Policy the reserve vector, widths, and required totals come from.
    policy: ScribeArtifactPolicy,
    /// Pod capacity each category is checked against.
    capacity: ScribeGlobalCapacity,
    /// Installed tenant cells.
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

    /// Returns the table width every activating tenant reserves atomically.
    #[must_use]
    pub const fn tenant_table_width(&self) -> usize {
        self.policy.geometry().guaranteed_active_tables_per_tenant()
    }

    /// Returns the complete vectors held for the pod's whole life.
    ///
    /// This is the startup reservation, not a live count: it is held whether or
    /// not the guaranteed owners have arrived, which is what stops an early
    /// tenant from spending capacity a later guaranteed tenant is owed.
    #[must_use]
    pub const fn guaranteed_vectors(&self) -> usize {
        self.policy.geometry().guaranteed_width()
    }

    /// Returns the elastic surplus in one category.
    ///
    /// Capacity beyond the startup reservation and beyond any extra vectors
    /// owners have installed past their configured width. Everything below that
    /// line is a protected cell and is never borrowable.
    fn surplus(&self, state: &LedgerState, category: ContentionCategory) -> usize {
        let component = self.reserve_vector().component(category);
        let held = self
            .guaranteed_vectors()
            .max(state.reserved_vectors())
            .saturating_mul(component);
        self.capacity.component(category).saturating_sub(held)
    }

    /// Returns the pod-wide committed amount in one category.
    ///
    /// The permanently held reservation plus everything borrowed beyond it, so
    /// the value a widening or a borrow is checked against is always the same
    /// quantity the capacity bound describes.
    fn pod_committed(&self, state: &LedgerState, category: ContentionCategory) -> usize {
        let component = self.reserve_vector().component(category);
        self.guaranteed_vectors()
            .max(state.reserved_vectors())
            .saturating_mul(component)
            .saturating_add(state.borrowed(&self.reserve_vector(), category))
    }

    /// Installs one table's cell, activating its tenant first if needed.
    ///
    /// Activation is hierarchical and all-or-nothing at each level. A tenant
    /// that is not yet active reserves its whole configured table width in every
    /// category at once, before it owns a single table, which is what keeps one
    /// tenant's arrival order from consuming the vectors another guaranteed
    /// tenant is entitled to. A table inside an active tenant is free while the
    /// tenant still has an unused reserved vector, and otherwise widens the
    /// tenant by one complete vector only when that vector fits pod capacity in
    /// every category.
    ///
    /// Partial installation at either level is what would let a table
    /// acknowledge an append it cannot later stage, merge, or publish, so every
    /// category is checked before anything is committed. Activating an
    /// already-active table is idempotent and reserves nothing further.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::ActivationWidth`] naming the level and the
    /// first category whose capacity cannot hold the required reservation, and
    /// [`ContentionRefusal::Poisoned`] when the ledger lock is poisoned.
    pub fn activate(&self, key: &ContentionKey) -> Result<(), ContentionRefusal> {
        let width = self.tenant_table_width();
        let mut state = self.lock()?;

        let Some(tenant) = state.tenants.get(&key.tenant) else {
            // A brand new tenant takes its whole table width in one step.
            self.check_widening(&state, width, "tenant")?;
            let mut tenant = TenantScribeLedger {
                tables: HashMap::new(),
                reserved_vectors: width,
            };
            tenant
                .tables
                .insert(key.table.clone(), TenantTableLedger::default());
            state.tenants.insert(key.tenant, tenant);
            return Ok(());
        };

        if tenant.tables.contains_key(&key.table) {
            return Ok(());
        }
        let widen = tenant.tables.len() >= tenant.reserved_vectors;
        if widen {
            // Beyond the configured width: one complete extra vector or nothing.
            self.check_widening(&state, 1, "table")?;
        }
        let Some(tenant) = state.tenants.get_mut(&key.tenant) else {
            return Err(ContentionRefusal::Inactive);
        };
        if widen {
            tenant.reserved_vectors += 1;
        }
        tenant
            .tables
            .insert(key.table.clone(), TenantTableLedger::default());
        Ok(())
    }

    /// Verifies that `vectors` more complete reserve vectors fit every category.
    ///
    /// Checked against the pod total before anything is mutated, so a widening
    /// that fails in the last category leaves the tenant map untouched. Extra
    /// vectors come out of surplus: they can never displace the permanently held
    /// startup reservation.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::ActivationWidth`] naming `scope` and the
    /// first category that cannot hold the additional vectors.
    fn check_widening(
        &self,
        state: &LedgerState,
        vectors: usize,
        scope: &'static str,
    ) -> Result<(), ContentionRefusal> {
        let vector = self.reserve_vector();
        let held = state.reserved_vectors();
        let guaranteed = self.guaranteed_vectors();
        for category in ContentionCategory::ALL {
            let component = vector.component(category);
            let capacity = self.capacity.component(category);
            let committed = self.pod_committed(state, category);
            let next_held = guaranteed
                .max(held.saturating_add(vectors))
                .checked_mul(component)
                .and_then(|held| held.checked_add(state.borrowed(&vector, category)));
            if next_held.is_none_or(|next| next > capacity) {
                return Err(ContentionRefusal::ActivationWidth {
                    scope,
                    category: category.label(),
                    capacity,
                    committed,
                    active: state.active_tenants(),
                });
            }
        }
        Ok(())
    }

    /// Releases one table's cell and every category it still held.
    ///
    /// The tenant keeps its configured table width so a table that goes quiet
    /// for a moment does not surrender a protected cell to a hot neighbour; only
    /// vectors the tenant borrowed past that width are given back. The tenant
    /// cell itself is released once its last table is gone.
    ///
    /// Deactivating a table that is not active is a no-op rather than an error:
    /// the shard and admission owners may both reach the same conclusion about
    /// an idle table, and neither should fail because the other got there first.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::Poisoned`] when the ledger lock is poisoned.
    pub fn deactivate(&self, key: &ContentionKey) -> Result<(), ContentionRefusal> {
        let width = self.tenant_table_width();
        let mut state = self.lock()?;
        let Some(tenant) = state.tenants.get_mut(&key.tenant) else {
            return Ok(());
        };
        if tenant.tables.remove(&key.table).is_none() {
            return Ok(());
        }
        if tenant.tables.is_empty() {
            state.tenants.remove(&key.tenant);
            return Ok(());
        }
        // Shrink back toward the configured width, never below the tables that
        // are still installed.
        tenant.reserved_vectors = tenant.reserved_vectors.min(width.max(tenant.tables.len()));
        Ok(())
    }

    /// Charges `amount` in one category to an active table.
    ///
    /// A charge inside the table's protected reserve always succeeds and costs
    /// the pod nothing new, because that reserve was already committed when its
    /// tenant activated. A charge beyond the reserve borrows from the shared
    /// surplus and is bounded three ways: by the table's equal share of its
    /// tenant's share, by the tenant's equal share of pod surplus, and by pod
    /// capacity itself. Both shares are recomputed here from the live owner
    /// counts, so a hot table cannot hold a share a newly arrived neighbour is
    /// entitled to, and neither can reach into a protected cell.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::Inactive`] when the table holds no cell,
    /// [`ContentionRefusal::Exhausted`] naming the level whose protected reserve
    /// and soft share together cannot cover the request, and
    /// [`ContentionRefusal::Poisoned`] when the ledger lock is poisoned.
    pub fn charge(
        &self,
        key: &ContentionKey,
        category: ContentionCategory,
        amount: usize,
    ) -> Result<(), ContentionRefusal> {
        let vector = self.reserve_vector();
        let reserve = vector.component(category);
        let capacity = self.capacity.component(category);
        let mut state = self.lock()?;
        let surplus = self.surplus(&state, category);
        let pod_committed = self.pod_committed(&state, category);
        let active_tenants = state.active_tenants().max(1);

        let tenant = state
            .tenants
            .get(&key.tenant)
            .ok_or(ContentionRefusal::Inactive)?;
        let usage = tenant
            .tables
            .get(&key.table)
            .copied()
            .ok_or(ContentionRefusal::Inactive)?;
        let active_tables = tenant.tables.len().max(1);
        let tenant_protected = tenant.protected(&vector, category);
        let tenant_committed = tenant.committed(category);
        let tenant_borrowed = tenant.borrowed(&vector, category);

        // Independent equal soft shares. Surplus splits evenly across active
        // tenants, and a tenant's share of it splits evenly across its active
        // tables. A protected cell is a floor under the ceiling, never a cap,
        // and is never part of anyone else's share.
        let tenant_surplus_share = surplus / active_tenants;
        let tenant_ceiling = tenant_protected.saturating_add(tenant_surplus_share);
        let table_ceiling = reserve.saturating_add(tenant_surplus_share / active_tables);

        let exhausted =
            |scope: &'static str, protected: usize, ceiling: usize| ContentionRefusal::Exhausted {
                scope,
                category: category.label(),
                protected,
                ceiling,
                requested: amount,
            };
        let current = usage.get(category);
        let next_usage = current
            .checked_add(amount)
            .ok_or_else(|| exhausted("table", reserve, table_ceiling))?;
        if next_usage > table_ceiling {
            return Err(exhausted("table", reserve, table_ceiling));
        }
        let next_tenant = tenant_committed
            .checked_add(amount)
            .ok_or_else(|| exhausted("tenant", tenant_protected, tenant_ceiling))?;
        if next_tenant > tenant_ceiling {
            return Err(exhausted("tenant", tenant_protected, tenant_ceiling));
        }
        // Only growth beyond the tenant's own protected cell touches the pod.
        let extra = next_tenant.saturating_sub(tenant_protected) - tenant_borrowed;
        if pod_committed
            .checked_add(extra)
            .is_none_or(|next| next > capacity)
        {
            return Err(exhausted("table", reserve, table_ceiling));
        }

        if let Some(cell) = state
            .tenants
            .get_mut(&key.tenant)
            .and_then(|tenant| tenant.tables.get_mut(&key.table))
        {
            cell.set(category, next_usage);
        }
        Ok(())
    }

    /// Returns `amount` in one category to an active table's cell.
    ///
    /// Releasing more than the cell holds is an accounting bug rather than a
    /// caller error, so it saturates at zero instead of wrapping into an
    /// enormous usage that would lock the table out permanently. Pod and tenant
    /// totals are derived from the cell, so returning it here is the whole
    /// release.
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
        let mut state = self.lock()?;
        let cell = state
            .tenants
            .get_mut(&key.tenant)
            .and_then(|tenant| tenant.tables.get_mut(&key.table))
            .ok_or(ContentionRefusal::Inactive)?;
        let next_usage = cell.get(category).saturating_sub(amount);
        cell.set(category, next_usage);
        Ok(())
    }

    /// Returns the number of currently active table cells across every tenant.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::Poisoned`] when the ledger lock is poisoned.
    pub fn active_cells(&self) -> Result<usize, ContentionRefusal> {
        Ok(self
            .lock()?
            .tenants
            .values()
            .map(|tenant| tenant.tables.len())
            .sum())
    }

    /// Returns the number of currently active tenant cells.
    ///
    /// Exposed alongside [`Self::active_cells`] because the two widths are
    /// enforced independently: a pod can be at its tenant width with idle table
    /// slots, or at its table width inside a single tenant.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::Poisoned`] when the ledger lock is poisoned.
    pub fn active_tenants(&self) -> Result<usize, ContentionRefusal> {
        Ok(self.lock()?.active_tenants())
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
            .tenants
            .get(&key.tenant)
            .and_then(|tenant| tenant.tables.get(&key.table))
            .map(|usage| usage.get(category))
            .ok_or(ContentionRefusal::Inactive)
    }

    /// Returns one tenant's committed amount in one category.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::Inactive`] when the tenant holds no cell,
    /// and [`ContentionRefusal::Poisoned`] when the ledger lock is poisoned.
    pub fn tenant_usage(
        &self,
        tenant: &DataTenantId,
        category: ContentionCategory,
    ) -> Result<usize, ContentionRefusal> {
        let state = self.lock()?;
        state
            .tenants
            .get(tenant)
            .map(|cell| cell.committed(category))
            .ok_or(ContentionRefusal::Inactive)
    }

    /// Returns the pod-wide committed total in one category.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::Poisoned`] when the ledger lock is poisoned.
    pub fn committed(&self, category: ContentionCategory) -> Result<usize, ContentionRefusal> {
        let state = self.lock()?;
        Ok(self.pod_committed(&state, category))
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
    use crate::scribe::geometry::{
        DEFAULT_ACTIVE_GENERATION_BUDGET_BYTES, DEFAULT_GENERATION_MAX_AGE,
        DEFAULT_GENERATION_ROTATION_CEILING_BYTES, DEFAULT_MAXIMUM_ACTIVE_REQUEST_OWNERSHIP_BYTES,
        DEFAULT_MAXIMUM_IMMUTABLE_MEMBER_OWNERSHIP_BYTES, DEFAULT_MINIMUM_MERGE_LANE_SCRATCH_BYTES,
        DEFAULT_MINIMUM_STAGE_MEMBER_BYTES, DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES,
        DEFAULT_WAL_SEGMENT_BYTES, ScribeGeometry,
    };

    /// Builds a cell identity for one tenant ordinal and table name.
    ///
    /// # Panics
    ///
    /// Panics when the synthetic tenant identifier is not accepted, which would
    /// be a fixture bug rather than a ledger behavior.
    fn key(tenant: u8, table: &str) -> ContentionKey {
        ContentionKey::new(
            tenant_id(tenant),
            TableRef::new(BifrostNamespace::Datasets, table),
        )
    }

    /// Builds the synthetic tenant identifier for one tenant ordinal.
    ///
    /// # Panics
    ///
    /// Panics when the synthetic identifier is not a valid `UUIDv7` tenant.
    fn tenant_id(tenant: u8) -> DataTenantId {
        let mut bytes = [tenant; 16];
        // Force the UUIDv7 version and variant nibbles the identifier requires.
        bytes[6] = 0x70 | (bytes[6] & 0x0f);
        bytes[8] = 0x80 | (bytes[8] & 0x3f);
        DataTenantId::new(uuid::Uuid::from_bytes(bytes)).expect("UUIDv7 test tenant")
    }

    /// Builds a ledger over an explicit guaranteed width plus surplus vectors.
    ///
    /// Capacity is `(tenants × tables + surplus) × vector` in every category, so
    /// a test states the exact protected width it wants and exactly how much
    /// elastic room sits beyond it.
    ///
    /// # Panics
    ///
    /// Panics when the requested widths do not form a valid geometry, which
    /// would be a fixture bug rather than a ledger behavior.
    fn ledger_for(tenants: usize, tables: usize, surplus: usize) -> ScribeContentionLedger {
        let geometry = ScribeGeometry::new(
            DEFAULT_WAL_SEGMENT_BYTES,
            DEFAULT_ACTIVE_GENERATION_BUDGET_BYTES,
            DEFAULT_GENERATION_ROTATION_CEILING_BYTES,
            DEFAULT_GENERATION_MAX_AGE,
            None,
            None,
            DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES,
            tenants,
            tables,
            crate::gate::limits::BIFROST_INGEST_REQUEST_LIMIT_BYTES,
            DEFAULT_MAXIMUM_ACTIVE_REQUEST_OWNERSHIP_BYTES,
            DEFAULT_MAXIMUM_IMMUTABLE_MEMBER_OWNERSHIP_BYTES,
            DEFAULT_MINIMUM_STAGE_MEMBER_BYTES,
            DEFAULT_MINIMUM_MERGE_LANE_SCRATCH_BYTES,
        )
        .expect("test geometry must be valid");
        let policy = ScribeArtifactPolicy::new(geometry);
        let vector = policy.reserve_vector();
        let vectors = tenants * tables + surplus;
        let capacity = ScribeGlobalCapacity {
            admission_items: vector.admission_items * vectors,
            admission_bytes: vector.admission_bytes * vectors,
            active_bytes: vector.active_bytes * vectors,
            immutable_bytes: vector.immutable_bytes * vectors,
            durable_stage_bytes: vector.durable_stage_bytes * vectors,
            merge_scratch_bytes: vector.merge_scratch_bytes * vectors,
            staging_claim_items: vector.staging_claim_items * vectors,
            upload_claim_items: vector.upload_claim_items * vectors,
        };
        policy
            .validate_capacity(&capacity)
            .expect("a fixture capacity must hold the guaranteed width it declares");
        ScribeContentionLedger::new(policy, capacity)
    }

    /// One tenant's arrival order cannot consume another tenant's protected width.
    ///
    /// # Panics
    ///
    /// Panics when the first tenant borrows past its own protected cell on a pod
    /// with no surplus, or when a later guaranteed tenant is then refused the
    /// width the pod permanently reserved for it.
    #[test]
    fn a_first_tenant_cannot_consume_the_width_of_later_guaranteed_tenants() {
        // The default guaranteed geometry with no surplus at all: every byte of
        // capacity belongs to a protected cell.
        let ledger = ledger_for(4, 2, 0);
        let hot = key(1, "hot");
        ledger.activate(&hot).expect("first tenant activates");

        let reserve = ledger.reserve_vector().active_bytes;
        ledger
            .charge(&hot, ContentionCategory::Active, reserve)
            .expect("a table always gets its own protected reserve");
        assert!(
            ledger.charge(&hot, ContentionCategory::Active, 1).is_err(),
            "with no surplus, one tenant may not borrow a byte it does not own"
        );

        for tenant in 2..=4_u8 {
            for table in ["system", "dynamic"] {
                ledger
                    .activate(&key(tenant, table))
                    .unwrap_or_else(|error| {
                        panic!("tenant {tenant}/{table} must activate: {error}")
                    });
            }
        }
        let late = key(4, "dynamic");
        ledger
            .charge(&late, ContentionCategory::Active, reserve)
            .expect("a protected cell is untouched by an earlier hot tenant");
    }

    /// A hot table drains rather than keeps its sibling's share when it arrives.
    ///
    /// # Panics
    ///
    /// Panics when the hot table keeps borrowing after its share halves, when
    /// share shrink revokes what it already holds, or when the sibling is then
    /// refused its protected reserve.
    #[test]
    fn a_hot_table_cannot_consume_its_sibling_system_table_vector() {
        // One tenant of two tables, with two spare vectors of surplus.
        let ledger = ledger_for(1, 2, 2);
        let dynamic = key(1, "dynamic");
        let system = key(1, "system");
        ledger.activate(&dynamic).expect("dynamic table activates");

        let reserve = ledger.reserve_vector().active_bytes;
        // Its own reserve plus the whole surplus: two spare vectors.
        ledger
            .charge(&dynamic, ContentionCategory::Active, reserve * 3)
            .expect("the only active table may borrow the whole surplus");
        assert!(
            ledger
                .charge(&dynamic, ContentionCategory::Active, 1)
                .is_err(),
            "surplus is finite; the next byte would come out of a protected cell"
        );

        // The sibling arrives, halving the surplus share.
        ledger.activate(&system).expect("sibling table activates");
        assert!(
            ledger
                .charge(&dynamic, ContentionCategory::Active, 1)
                .is_err(),
            "an over-share owner is refused new borrowing once a sibling arrives"
        );
        assert_eq!(
            ledger
                .usage(&dynamic, ContentionCategory::Active)
                .expect("the over-share owner keeps what it holds"),
            reserve * 3,
            "share shrink drains borrowers, it never revokes ownership"
        );
        ledger
            .charge(&system, ContentionCategory::Active, reserve)
            .expect("a protected sibling cell is never borrowable");
    }

    /// Surplus rebalances equally per category as tenants arrive.
    ///
    /// # Panics
    ///
    /// Panics when a share is not recomputed from the live tenant count, or when
    /// one category's exhaustion leaks into another.
    #[test]
    fn soft_shares_rebalance_equally_and_independently_per_category() {
        // Two guaranteed tenants of two tables, plus four surplus vectors.
        let ledger = ledger_for(2, 2, 4);
        let first = key(1, "t");
        let second = key(2, "t");
        ledger.activate(&first).expect("first tenant activates");

        let vector = ledger.reserve_vector();
        // Alone, the tenant's share is the whole surplus, and its one table's
        // share of that is the whole thing again.
        ledger
            .charge(&first, ContentionCategory::Active, vector.active_bytes * 5)
            .expect("a lone tenant may borrow the whole surplus");
        // Exhausting `active` says nothing about `immutable`.
        ledger
            .charge(
                &first,
                ContentionCategory::Immutable,
                vector.immutable_bytes * 5,
            )
            .expect("categories are governed independently");

        ledger.activate(&second).expect("second tenant activates");
        assert!(
            ledger
                .charge(&second, ContentionCategory::Active, vector.active_bytes * 4)
                .is_err(),
            "the arriving tenant is bounded by its own equal half-share"
        );
        ledger
            .charge(&second, ContentionCategory::Active, vector.active_bytes)
            .expect("its protected reserve is always available");
    }

    /// The pod refuses tenants and tables beyond the width its capacity holds.
    ///
    /// # Panics
    ///
    /// Panics when activation over-commits, when the refusal does not name the
    /// exhausted level and category, or when releasing does not free the cell.
    #[test]
    fn activation_is_bounded_at_both_levels_and_returns_its_vectors_on_release() {
        // Room for exactly two tenants of two tables and nothing more.
        let ledger = ledger_for(2, 2, 0);
        ledger.activate(&key(1, "a")).expect("first tenant");
        ledger.activate(&key(2, "a")).expect("second tenant");
        let refusal = ledger
            .activate(&key(3, "a"))
            .expect_err("a third tenant exceeds the pod's activation width");
        let ContentionRefusal::ActivationWidth {
            scope,
            category,
            active,
            ..
        } = refusal
        else {
            panic!("an over-wide activation must report the activation diagnostic");
        };
        assert_eq!(scope, "tenant");
        assert_eq!(active, 2);
        assert_eq!(category, ContentionCategory::AdmissionItems.label());

        // The second table inside an active tenant is already paid for.
        ledger
            .activate(&key(1, "b"))
            .expect("reserved second table");
        // A third table in that tenant needs a whole extra vector the pod lacks.
        let refusal = ledger
            .activate(&key(1, "c"))
            .expect_err("a third table exceeds the tenant's reserved width");
        let ContentionRefusal::ActivationWidth { scope, .. } = refusal else {
            panic!("an over-wide table activation must report the diagnostic");
        };
        assert_eq!(scope, "table");

        // Releasing a tenant's last table returns its whole cell.
        ledger.deactivate(&key(2, "a")).expect("release");
        assert_eq!(
            ledger.active_tenants().expect("tenant count"),
            1,
            "an empty tenant releases its reserved width"
        );
        ledger
            .activate(&key(3, "a"))
            .expect("the freed width is reusable");
    }

    /// Activating an already-active table reserves nothing further.
    ///
    /// # Panics
    ///
    /// Panics when a repeated activation double-commits its tenant's width.
    #[test]
    fn repeated_activation_is_idempotent() {
        let ledger = ledger_for(2, 2, 0);
        let table = key(1, "a");
        ledger.activate(&table).expect("first activation");
        let committed = ledger
            .committed(ContentionCategory::Active)
            .expect("committed total");
        ledger.activate(&table).expect("second activation");
        assert_eq!(
            ledger
                .committed(ContentionCategory::Active)
                .expect("committed total"),
            committed,
            "an idempotent activation must not re-reserve the tenant width"
        );
        assert_eq!(
            ledger.active_cells().expect("cell count"),
            1,
            "an idempotent activation installs no second cell"
        );
    }

    /// Released surplus returns to the pod and is borrowable again.
    ///
    /// # Panics
    ///
    /// Panics when release does not return borrowed surplus to the pod total.
    #[test]
    fn released_surplus_returns_to_the_pod_total() {
        let ledger = ledger_for(1, 2, 2);
        let table = key(1, "a");
        ledger.activate(&table).expect("activation");
        let vector = ledger.reserve_vector();
        let protected = vector.active_bytes * 2;
        ledger
            .charge(&table, ContentionCategory::Active, vector.active_bytes * 3)
            .expect("borrow the whole surplus");
        assert_eq!(
            ledger
                .committed(ContentionCategory::Active)
                .expect("committed"),
            protected + vector.active_bytes,
            "the pod holds its protected width plus the borrowed surplus"
        );
        ledger
            .release(&table, ContentionCategory::Active, vector.active_bytes * 2)
            .expect("release the borrowed part");
        assert_eq!(
            ledger
                .committed(ContentionCategory::Active)
                .expect("committed"),
            protected,
            "returning borrowed surplus leaves only the protected width"
        );
        assert_eq!(
            ledger
                .usage(&table, ContentionCategory::Active)
                .expect("usage"),
            vector.active_bytes
        );
    }
}
