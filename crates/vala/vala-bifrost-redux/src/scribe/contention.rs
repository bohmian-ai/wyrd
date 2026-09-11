//! Per-tenant, per-table contention accounting for Scribe.
//!
//! Pod-global counters alone cannot keep one hot table from starving its
//! neighbours: a tenant that saturates the global in-flight ceiling makes every
//! other tenant's append fail with the same busy error, and nothing in a global
//! counter says whose fault that is or whose work should have been protected.
//!
//! This module adds the two missing dimensions without inventing tenants or
//! tables that do not exist. Nothing here is sized by a tenant count or a table
//! count. Capacity is whatever the node actually measured, owners are created
//! lazily from real traffic, and how many owners the pod can carry is derived
//! from that capacity by [`ScribeArtifactPolicy::max_active_tables`].
//!
//! Three rules govern every decision:
//!
//! - **Lazy ownership.** A tenant cell and a table cell exist only while that
//!   tenant and table are actually active or actually contending. No cell is
//!   held for an identity that has never sent traffic, so one tenant with one
//!   table may use every genuinely idle byte, item and lane the pod owns.
//! - **Dynamic max-min shares.** A ceiling is never a fixed division of
//!   capacity. On every charge the ledger recomputes the max-min fair level
//!   across the live owners, so an owner below its level frees the remainder for
//!   everyone else and a hungry owner rises only to the level fairness allows.
//!   Levels are computed independently per category: byte, item and lane
//!   categories are never summed or traded against one another.
//! - **Acknowledged work is never revoked.** When a new contender shrinks
//!   everyone's level, an owner already above it keeps what it holds and drains
//!   it through the normal lifecycle. It is refused only *new* growth. That is
//!   the difference between backpressure and data loss.
//!
//! Fair *ordering* needs one more thing: a contender that was refused must get
//! the next released capacity before an incumbent can take it back. That is what
//! [`DemandRecord`] is for. A demand record is identity-only — a tenant and a
//! registered canonical table — bounded by a configured queue capacity,
//! expiring, and removed on admission, cancellation or expiry. It owns no bytes,
//! writes no WAL, creates no durable row, and never appears in a metric label.
//!
//! Pod-wide and tenant-wide totals are derived from the owner map on every read
//! rather than cached beside it. A cached total is one more thing that can drift
//! from the cells it is supposed to summarize.
//!
//! The ledger owns the accounting only. It does not perform IO, does not know
//! what a batch is, and does not decide when a table stops being active; those
//! belong to the admission and shard owners that hold it.

use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use wyrd_spec::ids::DataTenantId;

use crate::catalog::TableRef;
use crate::contracts::ScribeError;
use crate::scribe::geometry::{
    ContentionCategory, ContentionReserveVector, ScribeArtifactPolicy, ScribeGlobalCapacity,
};
use crate::scribe::telemetry::{
    ContentionEffect, ContentionFacts, ScribeTelemetry, ScribeTelemetrySnapshot,
};

/// Live contention-demand records the pod will hold before refusing to queue.
///
/// The queue is an operational bound, not a tenant or table cardinality: it caps
/// how many identity-only waiting records the pod tracks at once, and a full
/// queue simply means a refused contender is not promised ordering, never that
/// it is denied service once capacity frees.
pub const DEFAULT_CONTENTION_DEMAND_CAPACITY: usize = 1_024;

/// How long one identity-only contention-demand record stays live.
///
/// A contender that has gone away must not hold ordering priority forever, so a
/// record expires rather than requiring an explicit cancellation from a caller
/// that may already have failed.
pub const DEFAULT_CONTENTION_DEMAND_TTL: Duration = Duration::from_secs(30);

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

/// One bounded, expiring record that a real table is waiting for capacity.
///
/// Holds identity, what it is waiting for, and an expiry, and nothing else. It
/// exists so a contender that was refused is served before an incumbent can
/// reborrow the capacity it releases, and it is removed the moment that
/// contender succeeds, cancels, or expires.
///
/// The two kinds of waiting are distinct and both are needed. A record with no
/// category is waiting to *activate*: it wants a whole lifecycle vector and
/// stops mattering the moment its table becomes active. A record with a
/// category belongs to a table that is already active and was refused capacity
/// in that one category; it must survive precisely because its table is active,
/// since that is the case an over-share incumbent would otherwise reacquire
/// past.
#[derive(Debug, Clone)]
struct DemandRecord {
    /// Which registered canonical table is waiting.
    key: ContentionKey,
    /// The category it was refused in, or `None` when it is waiting to activate.
    category: Option<ContentionCategory>,
    /// When this record stops conferring ordering priority.
    expires_at: Instant,
}

impl DemandRecord {
    /// Returns whether this record competes for capacity in one category.
    ///
    /// An activation record competes in every category, because the table it
    /// speaks for needs a complete lifecycle vector before it can serve.
    const fn competes_in(&self, category: ContentionCategory) -> bool {
        match self.category {
            None => true,
            Some(waiting) => waiting as usize == category as usize,
        }
    }
}

/// What one call to [`ScribeContentionLedger::activate`] actually did.
///
/// The caller needs this to undo a failed admission: only a cell this call
/// created may be rolled back, because rolling back a cell that was already
/// serving would revoke another caller's acknowledged work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationOutcome {
    /// This call installed the table's cell.
    Created,
    /// The table already held a cell; nothing was installed.
    AlreadyActive,
}

/// Live usage of one active canonical table across every governed category.
///
/// This is the innermost ledger in the hierarchy. It holds only committed
/// amounts; every ceiling it is checked against is recomputed from the live
/// owner set on each charge, because a cached ceiling is exactly what would let
/// an owner keep a share a newly arrived neighbour is entitled to.
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

/// One tenant's lazily created cell and the canonical tables nested inside it.
///
/// The cell exists only while the tenant has at least one active table. It
/// reserves nothing on its own: a tenant that stops writing leaves no permanent
/// claim on capacity behind it.
#[derive(Debug, Default)]
struct TenantScribeLedger {
    /// Nested canonical-table ledgers by table identity.
    tables: HashMap<TableRef, TenantTableLedger>,
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
}

/// One in-progress charge, as every bound in the ledger sees it.
///
/// Carried as one value rather than five parameters because the bounds are
/// evaluated in sequence and each must see exactly the same request; threading
/// the parts separately is how they would drift apart.
#[derive(Debug, Clone, Copy)]
struct ChargeRequest<'a> {
    /// Who is charging.
    key: &'a ContentionKey,
    /// Which governed category the charge lands in.
    category: ContentionCategory,
    /// How much the caller asked for.
    amount: usize,
    /// The lifecycle vector's component in this category.
    quantum: usize,
}

/// Mutable ledger state guarded by one lock.
///
/// Owners and waiting demand move together under the same lock so a concurrent
/// activation can never observe a queue the owner map does not yet agree with,
/// which would let the pod over-commit by exactly one table.
#[derive(Debug, Default)]
struct LedgerState {
    /// Installed tenant cells by identity.
    tenants: HashMap<DataTenantId, TenantScribeLedger>,
    /// Identity-only waiting records in arrival order.
    demand: VecDeque<DemandRecord>,
}

impl LedgerState {
    /// Returns the number of active tenants sharing every category.
    fn active_tenants(&self) -> usize {
        self.tenants.len()
    }

    /// Returns the number of active canonical tables across every tenant.
    fn active_tables(&self) -> usize {
        self.tenants
            .values()
            .map(|tenant| tenant.tables.len())
            .sum()
    }

    /// Returns whether one table currently holds a cell.
    fn is_active(&self, key: &ContentionKey) -> bool {
        self.tenants
            .get(&key.tenant)
            .is_some_and(|tenant| tenant.tables.contains_key(&key.table))
    }

    /// Drops every demand record that expired or was overtaken by admission.
    ///
    /// Called at the head of every operation that reads demand so an abandoned
    /// contender cannot hold ordering priority. Only *activation* records are
    /// retired by their table becoming active; a category record belongs to an
    /// already-active table by definition, and pruning it on that basis is
    /// exactly what would let an over-share incumbent reacquire past it.
    fn prune_demand(&mut self, now: Instant) -> usize {
        let admitted: Vec<ContentionKey> = self
            .demand
            .iter()
            .filter(|record| record.category.is_none() && self.is_active(&record.key))
            .map(|record| record.key.clone())
            .collect();
        let expired = self
            .demand
            .iter()
            .filter(|record| record.expires_at <= now)
            .count();
        self.demand.retain(|record| {
            record.expires_at > now
                && !(record.category.is_none() && admitted.contains(&record.key))
        });
        expired
    }

    /// Returns the pod-wide committed amount in one category.
    ///
    /// Summed from the live cells on demand so a published total can never
    /// drift from the owners it summarizes.
    fn pod_committed(&self, category: ContentionCategory) -> usize {
        self.tenants
            .values()
            .map(|tenant| tenant.committed(category))
            .sum()
    }

    /// Returns how many distinct tables currently hold a demand record.
    ///
    /// Counted rather than tracked so the gauge one transition publishes can
    /// never disagree with the queue it summarizes.
    fn distinct_demand(&self) -> usize {
        let mut seen: Vec<&ContentionKey> = Vec::new();
        for record in &self.demand {
            if !seen.contains(&&record.key) {
                seen.push(&record.key);
            }
        }
        seen.len()
    }

    /// Returns the distinct tables waiting to activate ahead of `key`.
    ///
    /// A contender that already holds a record counts only the records ahead of
    /// its own; one that holds none counts every live record, because it is
    /// arriving behind all of them.
    fn waiting_ahead(&self, key: &ContentionKey) -> usize {
        self.distinct_ahead(key, |record| record.category.is_none())
    }

    /// Returns the distinct unserved contenders holding capacity back from `key`.
    ///
    /// A contender is *unserved* while its table holds less than one lifecycle
    /// quantum in the category it is waiting on: it has been refused and has not
    /// yet received enough to carry a table through. Only those reserve.
    ///
    /// Who counts as "ahead" depends on the caller. An unserved caller is a
    /// contender itself, so it only yields to the contenders queued before it
    /// and receives released capacity in queue order. A caller that is already
    /// served -- the over-share incumbent -- yields to *every* unserved
    /// contender regardless of when it queued, which is what makes reacquisition
    /// impossible while anyone is still waiting.
    fn reserved_ahead(
        &self,
        key: &ContentionKey,
        category: ContentionCategory,
        quantum: usize,
    ) -> usize {
        let caller_is_contender = self.unserved(key, category, quantum);
        let mut seen: Vec<&ContentionKey> = Vec::new();
        for record in &self.demand {
            if &record.key == key {
                if caller_is_contender {
                    break;
                }
                continue;
            }
            if record.competes_in(category)
                && self.unserved(&record.key, category, quantum)
                && !seen.contains(&&record.key)
            {
                seen.push(&record.key);
            }
        }
        seen.len()
    }

    /// Returns whether one table still holds less than a lifecycle quantum.
    fn unserved(&self, key: &ContentionKey, category: ContentionCategory, quantum: usize) -> bool {
        self.held(key, category) < quantum
    }

    /// Returns one table's committed amount, or zero when it holds no cell.
    fn held(&self, key: &ContentionKey, category: ContentionCategory) -> usize {
        self.tenants
            .get(&key.tenant)
            .and_then(|tenant| tenant.tables.get(&key.table))
            .map_or(0, |cell| cell.get(category))
    }

    /// Returns whether one table is queued for capacity in one category.
    fn has_demand(&self, key: &ContentionKey, category: ContentionCategory) -> bool {
        self.demand
            .iter()
            .any(|record| &record.key == key && record.competes_in(category))
    }

    /// Returns what one table effectively wants in a category right now.
    ///
    /// Its committed amount, plus the caller's request when it *is* the caller,
    /// plus the rest of a lifecycle quantum when it is queued and still
    /// unserved. That last term is what makes a refused contender visible to the
    /// fairness computation at all: without it a contender that holds nothing
    /// reads as wanting nothing, and every incumbent would be levelled against
    /// zero.
    fn effective_demand(
        &self,
        owner: &ContentionKey,
        held: usize,
        request: &ChargeRequest<'_>,
    ) -> usize {
        if owner == request.key {
            return held.saturating_add(request.amount);
        }
        if self.has_demand(owner, request.category) {
            return held.max(request.quantum);
        }
        held
    }

    /// Returns every live tenant's effective demand, and the caller's own.
    fn tenant_demands(&self, request: &ChargeRequest<'_>) -> (Vec<usize>, usize) {
        let mut demands = Vec::with_capacity(self.tenants.len());
        let mut charging = 0;
        for (tenant, cell) in &self.tenants {
            let demand: usize = cell
                .tables
                .iter()
                .map(|(table, usage)| {
                    let owner = ContentionKey::new(*tenant, table.clone());
                    self.effective_demand(&owner, usage.get(request.category), request)
                })
                .sum();
            if tenant == &request.key.tenant {
                charging = demand;
            }
            demands.push(demand);
        }
        (demands, charging)
    }

    /// Returns the charging tenant's table demands, the caller's own, and its held amount.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::Inactive`] when the charging tenant lost its
    /// cell between checks, which the caller treats as an inactive table.
    fn table_demands(
        &self,
        request: &ChargeRequest<'_>,
    ) -> Result<(Vec<usize>, usize, usize), ContentionRefusal> {
        let tenant = self
            .tenants
            .get(&request.key.tenant)
            .ok_or(ContentionRefusal::Inactive)?;
        let mut demands = Vec::with_capacity(tenant.tables.len());
        let mut charging = 0;
        let mut held = 0;
        for (table, cell) in &tenant.tables {
            let owner = ContentionKey::new(request.key.tenant, table.clone());
            let usage = cell.get(request.category);
            let demand = self.effective_demand(&owner, usage, request);
            if table == &request.key.table {
                held = usage;
                charging = demand;
            }
            demands.push(demand);
        }
        Ok((demands, charging, held))
    }

    /// Counts distinct matching demand keys registered before `key`'s own record.
    fn distinct_ahead(
        &self,
        key: &ContentionKey,
        matches: impl Fn(&DemandRecord) -> bool,
    ) -> usize {
        let mut seen: Vec<&ContentionKey> = Vec::new();
        for record in &self.demand {
            if &record.key == key {
                break;
            }
            if matches(record) && !seen.contains(&&record.key) {
                seen.push(&record.key);
            }
        }
        seen.len()
    }
}

/// Returns the max-min fair level for the unsatisfied owners of one category.
///
/// Classic progressive filling: every owner whose demand is at or below the
/// running equal share is satisfied outright, its unused remainder returns to
/// the pool, and the pool is redivided among those still hungry. The value
/// returned is the level a still-hungry owner may rise to. When every owner fits
/// inside capacity there is no binding level and [`usize::MAX`] is returned,
/// which is what makes the ledger work-conserving: one tenant with one table
/// faces no ceiling below the pod's real capacity.
///
/// `demands` is consumed as a scratch buffer and left sorted.
fn max_min_level(capacity: usize, demands: &mut [usize]) -> usize {
    demands.sort_unstable();
    let mut remaining = capacity;
    let mut hungry = demands.len();
    for demand in demands.iter() {
        if hungry == 0 {
            break;
        }
        let level = remaining / hungry;
        if *demand > level {
            return level;
        }
        remaining -= *demand;
        hungry -= 1;
    }
    usize::MAX
}

/// Why one contention charge or activation was refused.
///
/// The variants distinguish the refusals a caller must handle differently: an
/// owner that has reached its dynamic fair level is busy and may retry as soon
/// as any owner drains, while a pod already carrying every table its measured
/// resources can complete is at its derived ownership ceiling.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContentionRefusal {
    /// The pod cannot carry another canonical table through its lifecycle.
    #[error(
        "Scribe cannot activate another table: measured capacity completes {ceiling} tables, {active} are active and {waiting} contenders are already queued"
    )]
    OwnershipCeiling {
        /// Tables the pod's measured resources can complete simultaneously.
        ceiling: usize,
        /// Tables currently active.
        active: usize,
        /// Distinct contenders queued ahead of this one.
        waiting: usize,
    },
    /// One owner reached its dynamically recomputed max-min fair level.
    #[error(
        "Scribe {scope} contention limit reached for `{category}`: fair level {ceiling} cannot cover {held} plus {requested}"
    )]
    Exhausted {
        /// Which level the charge ran into: the pod, the tenant or the table.
        scope: &'static str,
        /// Fixed-cardinality category label that ran out.
        category: &'static str,
        /// The owner's max-min fair level in that category.
        ceiling: usize,
        /// What the owner already holds in that category.
        held: usize,
        /// Amount the caller asked for.
        requested: usize,
    },
    /// A charge referenced a tenant or table that holds no installed cell.
    #[error("Scribe contention charge for an inactive table")]
    Inactive,
    /// A release asked for more than the table's cell actually holds.
    #[error(
        "Scribe contention over-release of `{category}`: the cell holds {held} and {requested} was returned"
    )]
    OverRelease {
        /// Fixed-cardinality category label the release named.
        category: &'static str,
        /// What the cell actually holds in that category.
        held: usize,
        /// What the caller tried to return.
        requested: usize,
    },
    /// A table was deactivated while it still holds capacity.
    #[error("Scribe cannot deactivate a table still holding {held} in `{category}`")]
    NonEmpty {
        /// Fixed-cardinality category label still holding capacity.
        category: &'static str,
        /// What the cell still holds in that category.
        held: usize,
    },
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
    /// Reaching a fair level or the derived ownership ceiling is backpressure,
    /// so both surface as `IngestBusy`. The remaining variants are accounting
    /// invariants, not caller-visible pressure, so they surface as `Internal`.
    fn from(refusal: ContentionRefusal) -> Self {
        match refusal {
            ContentionRefusal::OwnershipCeiling { .. } | ContentionRefusal::Exhausted { .. } => {
                Self::IngestBusy {
                    table: refusal.to_string(),
                }
            }
            ContentionRefusal::Inactive
            | ContentionRefusal::OverRelease { .. }
            | ContentionRefusal::NonEmpty { .. }
            | ContentionRefusal::Poisoned { .. } => Self::Internal {
                detail: refusal.to_string(),
            },
        }
    }
}

/// What one terminal settlement attempt did to the owner map.
///
/// Distinguished so the settlement owner can report the tenant's own retirement
/// exactly once rather than inferring it from a table count a caller would have
/// to read back under a second lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Settlement {
    /// The table held no cell; nothing was tracked and nothing was removed.
    Untracked,
    /// The empty table cell was removed and its tenant still has other tables.
    TableRetired {
        /// Whether stale demand records went with the retired table.
        invalidated: bool,
    },
    /// The empty table cell was removed and it was its tenant's last.
    TenantRetired {
        /// Whether stale demand records went with the retired table.
        invalidated: bool,
    },
}

impl Settlement {
    /// Builds the retirement outcome for a table that was removed.
    const fn retired(was_last_table: bool, invalidated: bool) -> Self {
        if was_last_table {
            Self::TenantRetired { invalidated }
        } else {
            Self::TableRetired { invalidated }
        }
    }

    /// Returns the registry effect this settlement outcome is reported as.
    ///
    /// `Untracked` shares the table effect because both callers return before
    /// emitting anything for it: nothing was tracked, so nothing was settled.
    const fn effect(self) -> ContentionEffect {
        match self {
            Self::Untracked | Self::TableRetired { .. } => ContentionEffect::TableSettled,
            Self::TenantRetired { .. } => ContentionEffect::TenantSettled,
        }
    }

    /// Reports whether retirement also dropped stale demand records.
    const fn invalidated_demand(self) -> bool {
        match self {
            Self::Untracked => false,
            Self::TableRetired { invalidated } | Self::TenantRetired { invalidated } => invalidated,
        }
    }
}

/// Pod-wide tenant-keyed contention ledger.
///
/// Constructed once from the validated [`ScribeArtifactPolicy`] and the pod
/// capacity that startup already proved holds at least one complete lifecycle
/// vector, so the ledger never has to re-derive or re-validate either.
#[derive(Debug)]
pub struct ScribeContentionLedger {
    /// Policy the lifecycle reserve vector comes from.
    policy: ScribeArtifactPolicy,
    /// Pod capacity each category is checked against.
    capacity: ScribeGlobalCapacity,
    /// Tables the measured capacity can carry through their whole lifecycle.
    ownership_ceiling: usize,
    /// Live identity-only demand records the pod will hold at once.
    demand_capacity: usize,
    /// How long one demand record confers ordering priority.
    demand_ttl: Duration,
    /// Installed owners and waiting demand.
    state: Mutex<LedgerState>,
    /// The pod's single production observation owner for these transitions.
    telemetry: Arc<ScribeTelemetry>,
}

impl ScribeContentionLedger {
    /// Builds a ledger over one validated policy and the pod's real capacity.
    ///
    /// The ownership ceiling is derived here, once, from measured capacity
    /// divided by the lifecycle vector. It is the only table count in the
    /// module and it is never configured.
    #[must_use]
    pub fn new(policy: ScribeArtifactPolicy, capacity: ScribeGlobalCapacity) -> Self {
        Self::with_demand_bounds(
            policy,
            capacity,
            DEFAULT_CONTENTION_DEMAND_CAPACITY,
            DEFAULT_CONTENTION_DEMAND_TTL,
        )
    }

    /// Builds a ledger with explicit operational bounds on the demand queue.
    ///
    /// Exposed so an operator profile — and the tests that prove expiry and
    /// queue saturation — can state the bounds directly instead of waiting on
    /// wall-clock defaults.
    #[must_use]
    pub fn with_demand_bounds(
        policy: ScribeArtifactPolicy,
        capacity: ScribeGlobalCapacity,
        demand_capacity: usize,
        demand_ttl: Duration,
    ) -> Self {
        let ownership_ceiling = policy.max_active_tables(&capacity);
        Self {
            policy,
            capacity,
            ownership_ceiling,
            demand_capacity,
            demand_ttl,
            state: Mutex::new(LedgerState::default()),
            telemetry: Arc::new(ScribeTelemetry::default()),
        }
    }

    /// Returns the pod's observation owner for admission and contention effects.
    ///
    /// The admission controller and its in-flight reservations publish through
    /// this same instance so pod-global admission transitions and per-table
    /// contention transitions reconcile against one set of totals.
    pub(crate) fn telemetry(&self) -> &ScribeTelemetry {
        &self.telemetry
    }

    /// Returns a shared handle to the pod's one observation owner.
    ///
    /// The staged and claim lifecycle publishes through the same owner as
    /// admission does, so a pod has one set of reconcilable totals rather than
    /// one per subsystem. The ledger is the owner because it is built first and
    /// outlives every staging runtime the pod composes.
    #[must_use]
    pub(crate) fn telemetry_handle(&self) -> Arc<ScribeTelemetry> {
        Arc::clone(&self.telemetry)
    }

    /// Returns the reconcilable observation totals this ledger has published.
    ///
    /// Exposed so a caller can check the counters against the ledger's own live
    /// state -- installed minus released vectors against active cells, opened
    /// against closed admissions -- rather than trusting a counter as evidence
    /// that the transition it counts actually happened.
    #[must_use]
    pub fn telemetry_totals(&self) -> ScribeTelemetrySnapshot {
        self.telemetry.snapshot()
    }

    /// Returns the reconcilable staged and claim totals published so far.
    ///
    /// Exposed alongside [`Self::telemetry_totals`] so an operator surface can
    /// check the durable half of the pod the same way: staged minus retired
    /// members against the staged namespace's own count, and claims taken minus
    /// claims closed against the publications still in flight.
    #[must_use]
    pub fn staging_totals(&self) -> crate::scribe::telemetry::ScribeStagingSnapshot {
        self.telemetry.staging_snapshot()
    }

    /// Returns the lifecycle reserve vector one active table is sized against.
    #[must_use]
    pub const fn reserve_vector(&self) -> ContentionReserveVector {
        self.policy.reserve_vector()
    }

    /// Returns how many canonical tables the measured capacity can carry.
    ///
    /// Derived from resources at construction. It is not an entitlement and it
    /// is not divided between tenants: any tenant may hold all of it while the
    /// rest of the pod is idle.
    #[must_use]
    pub const fn ownership_ceiling(&self) -> usize {
        self.ownership_ceiling
    }

    /// Installs one table's cell, creating its tenant cell lazily if needed.
    ///
    /// Activation is allowed while the pod can still complete another table's
    /// whole lifecycle *and* every contender that queued earlier could be
    /// admitted first. The second condition is what stops an incumbent from
    /// reborrowing the slot a waiting contender is owed. A refusal registers the
    /// caller's bounded, expiring, identity-only demand record so its next
    /// attempt is ordered ahead of arrivals behind it.
    ///
    /// Activating an already-active table is idempotent and reserves nothing
    /// further.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::OwnershipCeiling`] when the pod cannot carry
    /// another table, or cannot carry it before the contenders already waiting,
    /// and [`ContentionRefusal::Poisoned`] when the ledger lock is poisoned.
    pub fn activate(&self, key: &ContentionKey) -> Result<ActivationOutcome, ContentionRefusal> {
        let mut state = self.lock()?;
        self.prune(&mut state);
        let outcome = self.install(&mut state, key)?;
        Self::retire_activation(&mut state, key);
        Ok(outcome)
    }

    /// Activates a table if needed and applies its initial admission charges
    /// as one indivisible ledger transition.
    ///
    /// This is the production admission entry point, and it exists because
    /// activation and the first charge cannot be two lock acquisitions. Between
    /// them the contender is installed, holds nothing, and -- if its ordering
    /// record were retired at activation -- would read as zero demand to every
    /// fairness computation. An incumbent above its recomputed share could take
    /// the capacity the contender had just been promised, and the contender
    /// would be refused and rolled back after waiting its whole turn. Holding
    /// one lock across install, both charges, demand retirement, and rollback
    /// closes that window entirely.
    ///
    /// The contender keeps its ordering claim for the whole transition. Demand
    /// retires only when both charges commit; a refusal removes every effect of
    /// this call and leaves a live demand record behind, so the next release
    /// still belongs to the contender rather than to whoever retries first.
    ///
    /// A table that was already active keeps its cell and everything it owns on
    /// refusal: the refusal describes this request, and says nothing about work
    /// the table is already carrying.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::OwnershipCeiling`] when the pod cannot carry
    /// another table before the contenders already waiting,
    /// [`ContentionRefusal::Exhausted`] when either initial charge exceeds a
    /// recomputed bound, and [`ContentionRefusal::Poisoned`] when the ledger
    /// lock is poisoned. Every refusal leaves the ledger exactly as it found it.
    pub fn admit(
        &self,
        key: &ContentionKey,
        bytes: usize,
    ) -> Result<ActivationOutcome, ContentionRefusal> {
        let mut state = self.lock()?;
        self.prune(&mut state);
        // Captured before anything moves. The item leg can legitimately satisfy
        // and retire an existing claim, and the byte leg can then refuse; the
        // table would be back at zero ownership but behind everyone it had been
        // ahead of. Rollback has to restore the claim, not just the balances.
        let claim = Self::claim_of(&state, key);
        let outcome = self.install(&mut state, key)?;
        if let Err(refusal) = self.charge_locked(
            &mut state,
            &self.request(key, ContentionCategory::AdmissionItems, 1),
        ) {
            self.roll_back(&mut state, key, outcome, claim);
            return Err(refusal);
        }
        if let Err(refusal) = self.charge_locked(
            &mut state,
            &self.request(key, ContentionCategory::AdmissionBytes, bytes),
        ) {
            // Returning the item charge cannot fail: this transition committed
            // it a moment ago under the same lock. A failure here is a ledger
            // invariant break, not a caller error, so it is reported as one and
            // the rollback still completes.
            if let Err(error) =
                Self::release_locked(&mut state, key, ContentionCategory::AdmissionItems, 1)
            {
                self.emit(
                    &state,
                    ContentionEffect::InvariantFailure,
                    ContentionFacts {
                        category: Some(ContentionCategory::AdmissionItems),
                        requested: 1,
                        ..ContentionFacts::default()
                    },
                );
                tracing::error!(error = %error, "admission item rollback broke ledger accounting");
            }
            self.roll_back(&mut state, key, outcome, claim);
            return Err(refusal);
        }
        Self::retire_activation(&mut state, key);
        if outcome == ActivationOutcome::Created {
            self.emit(
                &state,
                ContentionEffect::ActivationInstalled,
                ContentionFacts {
                    category: Some(ContentionCategory::AdmissionBytes),
                    requested: bytes,
                    held_after: state.held(key, ContentionCategory::AdmissionBytes),
                    ..ContentionFacts::default()
                },
            );
        }
        Ok(outcome)
    }

    /// Builds one charge request against this ledger's lifecycle vector.
    fn request<'a>(
        &self,
        key: &'a ContentionKey,
        category: ContentionCategory,
        amount: usize,
    ) -> ChargeRequest<'a> {
        ChargeRequest {
            key,
            category,
            amount,
            quantum: self.reserve_vector().component(category),
        }
    }

    /// Expires stale demand under an already-held lock and reports the effect.
    fn prune(&self, state: &mut LedgerState) {
        if state.prune_demand(Instant::now()) > 0 {
            self.emit(
                state,
                ContentionEffect::DemandExpired,
                ContentionFacts::default(),
            );
        }
    }

    /// Installs one table's cell under an already-held lock.
    ///
    /// The caller's ordering record is deliberately left in place: it is retired
    /// by [`Self::retire_activation`] only once the whole transition commits.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::OwnershipCeiling`] when the pod cannot carry
    /// another table, or cannot carry it before the contenders already waiting.
    fn install(
        &self,
        state: &mut LedgerState,
        key: &ContentionKey,
    ) -> Result<ActivationOutcome, ContentionRefusal> {
        if state.is_active(key) {
            return Ok(ActivationOutcome::AlreadyActive);
        }
        let active = state.active_tables();
        let waiting = state.waiting_ahead(key);
        if active.saturating_add(waiting) >= self.ownership_ceiling {
            self.enqueue_demand(state, key, None);
            self.emit(
                state,
                ContentionEffect::ActivationRefused,
                ContentionFacts {
                    ceiling: self.ownership_ceiling,
                    reserved_contenders: waiting,
                    ..ContentionFacts::default()
                },
            );
            return Err(ContentionRefusal::OwnershipCeiling {
                ceiling: self.ownership_ceiling,
                active,
                waiting,
            });
        }
        state
            .tenants
            .entry(key.tenant)
            .or_default()
            .tables
            .insert(key.table.clone(), TenantTableLedger::default());
        Ok(ActivationOutcome::Created)
    }

    /// Undoes everything one refused initial transition did to the ledger.
    ///
    /// Two things must come back. A cell *this* transition installed is removed
    /// -- a table that was already active keeps its cell, because rolling that
    /// back would revoke acknowledged work this request never owned. And the
    /// caller's demand claim is restored to the positions it held before the
    /// transition, because a leg that succeeded may already have retired a
    /// record the caller earned by waiting. Returning the balances without
    /// returning the queue position leaves the table at zero ownership and
    /// behind every rival it had been ahead of, which is the same reacquisition
    /// window the single lock exists to close.
    fn roll_back(
        &self,
        state: &mut LedgerState,
        key: &ContentionKey,
        outcome: ActivationOutcome,
        claim: Vec<(usize, DemandRecord)>,
    ) {
        if outcome == ActivationOutcome::Created {
            if let Some(tenant) = state.tenants.get_mut(&key.tenant) {
                tenant.tables.remove(&key.table);
                if tenant.tables.is_empty() {
                    state.tenants.remove(&key.tenant);
                }
            }
            // The contender is inactive again, so it needs a live activation
            // claim to keep the place in line it just spent its turn on.
            self.enqueue_demand(state, key, None);
            self.emit(
                state,
                ContentionEffect::ActivationRolledBack,
                ContentionFacts {
                    ceiling: self.ownership_ceiling,
                    ..ContentionFacts::default()
                },
            );
        }
        if Self::restore_claim(state, claim) {
            self.emit(
                state,
                ContentionEffect::DemandRestored,
                ContentionFacts::default(),
            );
        }
    }

    /// Returns one table's demand records with the queue positions they hold.
    ///
    /// Positions are captured because restoring a record at the back of the
    /// queue would hand the caller's earned priority to whoever queued after it.
    fn claim_of(state: &LedgerState, key: &ContentionKey) -> Vec<(usize, DemandRecord)> {
        state
            .demand
            .iter()
            .enumerate()
            .filter(|(_, record)| &record.key == key)
            .map(|(index, record)| (index, record.clone()))
            .collect()
    }

    /// Reinstates a captured claim and reports whether anything was returned.
    ///
    /// A record the transition never retired is left exactly as it is, including
    /// a refreshed expiry: only what went missing comes back, and it comes back
    /// at the index it occupied. Other tables' records are neither added nor
    /// removed by a refused transition, so those indices are still meaningful.
    fn restore_claim(state: &mut LedgerState, claim: Vec<(usize, DemandRecord)>) -> bool {
        let mut restored = false;
        for (index, record) in claim {
            if state.demand.iter().any(|live| {
                live.key == record.key
                    && live.category.map(|held| held as usize)
                        == record.category.map(|wanted| wanted as usize)
            }) {
                continue;
            }
            let at = index.min(state.demand.len());
            state.demand.insert(at, record);
            restored = true;
        }
        restored
    }

    /// Retires one table's activation claim once it is genuinely installed.
    fn retire_activation(state: &mut LedgerState, key: &ContentionKey) {
        state
            .demand
            .retain(|record| &record.key != key || record.category.is_some());
    }

    /// Publishes one registry effect with the ledger's live bounded state attached.
    ///
    /// The live gauges and the pod-wide committed total are read here rather
    /// than passed in, so every effect reports the state that actually exists at
    /// the moment it is published instead of whatever the call site last
    /// computed.
    fn emit(&self, state: &LedgerState, effect: ContentionEffect, facts: ContentionFacts) {
        self.telemetry.record(
            effect,
            ContentionFacts {
                pod_committed: facts
                    .category
                    .map_or(0, |category| state.pod_committed(category)),
                demand_records: state.distinct_demand(),
                active_tables: state.active_tables(),
                active_tenants: state.active_tenants(),
                ..facts
            },
        );
    }

    /// Records that one registered canonical table is waiting for capacity.
    ///
    /// Identity only: no bytes, no items, no lanes, no WAL, no durable row. A
    /// duplicate record is refreshed rather than appended so one caller cannot
    /// buy priority by retrying, and a full queue drops the request instead of
    /// growing without bound.
    fn enqueue_demand(
        &self,
        state: &mut LedgerState,
        key: &ContentionKey,
        category: Option<ContentionCategory>,
    ) {
        let expires_at = Instant::now() + self.demand_ttl;

        if let Some(record) = state.demand.iter_mut().find(|record| {
            &record.key == key
                && record.category.map(|held| held as usize)
                    == category.map(|wanted| wanted as usize)
        }) {
            // Refreshed in place, never re-appended: a caller that retries must
            // keep the queue position it earned rather than losing it to a
            // rival that has been waiting less time.
            record.expires_at = expires_at;
            self.emit(
                state,
                ContentionEffect::DemandRefreshed,
                ContentionFacts {
                    category,
                    ..ContentionFacts::default()
                },
            );
            return;
        }
        if state.demand.len() >= self.demand_capacity {
            self.emit(
                state,
                ContentionEffect::DemandDropped,
                ContentionFacts {
                    category,
                    ceiling: self.demand_capacity,
                    ..ContentionFacts::default()
                },
            );
            return;
        }
        state.demand.push_back(DemandRecord {
            key: key.clone(),
            category,
            expires_at,
        });
        self.emit(
            state,
            ContentionEffect::DemandEnqueued,
            ContentionFacts {
                category,
                ceiling: self.demand_capacity,
                ..ContentionFacts::default()
            },
        );
    }

    /// Withdraws every demand record one table holds.
    ///
    /// Called when a contender gives up or is invalidated, so it stops holding
    /// ordering priority ahead of callers that are still waiting. Both the
    /// activation record and any per-category records go together, because a
    /// caller that has given up is not waiting for any of them.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::Poisoned`] when the ledger lock is poisoned.
    pub fn cancel_demand(&self, key: &ContentionKey) -> Result<(), ContentionRefusal> {
        let mut state = self.lock()?;
        let before = state.demand.len();
        state.demand.retain(|record| &record.key != key);
        if state.demand.len() != before {
            self.emit(
                &state,
                ContentionEffect::DemandCancelled,
                ContentionFacts::default(),
            );
        }
        Ok(())
    }

    /// Returns how many distinct contenders are currently waiting.
    ///
    /// A count only. Tenant and table identities stay inside the ledger and
    /// never reach a metric label.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::Poisoned`] when the ledger lock is poisoned.
    pub fn waiting_demand(&self) -> Result<usize, ContentionRefusal> {
        let mut state = self.lock()?;
        self.prune(&mut state);
        Ok(state.distinct_demand())
    }

    /// Releases one table's cell once it holds nothing.
    ///
    /// Deactivation is refused while any category still carries a balance. A
    /// cell that vanished mid-flight would strand exactly that balance: the pod
    /// total would keep counting capacity no owner can ever return, and the
    /// table would have to reactivate into a pod that believes it is fuller than
    /// it is. Releasing everything first is the caller's job, and this refusal
    /// is how that obligation is enforced rather than assumed.
    ///
    /// A tenant whose last table is gone leaves no cell behind. The capacity
    /// freed here is what a queued contender takes next.
    ///
    /// Deactivating a table that is not active is a no-op rather than an error:
    /// the shard and admission owners may both reach the same conclusion about
    /// an idle table, and neither should fail because the other got there first.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::NonEmpty`] naming the first category that
    /// still holds capacity, leaving the ledger untouched, and
    /// [`ContentionRefusal::Poisoned`] when the ledger lock is poisoned.
    pub fn deactivate(&self, key: &ContentionKey) -> Result<(), ContentionRefusal> {
        let mut state = self.lock()?;
        match Self::retire_if_empty(&mut state, key) {
            Ok(Settlement::Untracked) => Ok(()),
            Ok(settlement) => {
                self.report_settlement(&state, settlement);
                Ok(())
            }
            Err(refusal) => {
                self.emit(
                    &state,
                    ContentionEffect::SettlementDeferred,
                    ContentionFacts::default(),
                );
                Err(refusal)
            }
        }
    }

    /// Settles one table at the end of its ownership and reports what it did.
    ///
    /// This is the production terminal-settlement owner. Normal release returns
    /// balances but leaves the cell installed, and an installed empty cell still
    /// occupies one of the pod's derived ownership slots. Sequential traffic
    /// across distinct tables would therefore walk the pod up to its ownership
    /// ceiling while owning nothing at all, and every later table would be
    /// refused against zero real usage. Calling this after every terminal
    /// release is what makes an idle table's slot genuinely reusable.
    ///
    /// Unlike [`Self::deactivate`], a table that still holds capacity is not an
    /// error here: settlement is evaluated after every release, and most
    /// releases legitimately leave work behind. The cell is removed exactly once
    /// and only when every governed category is zero; the tenant goes with its
    /// final table, and the settled table's stale demand records go with it.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::Poisoned`] when the ledger lock is poisoned.
    pub fn settle(&self, key: &ContentionKey) -> Result<bool, ContentionRefusal> {
        let mut state = self.lock()?;
        match Self::retire_if_empty(&mut state, key) {
            Ok(Settlement::Untracked) => Ok(false),
            Ok(settlement) => {
                self.report_settlement(&state, settlement);
                Ok(true)
            }
            Err(_) => {
                self.emit(
                    &state,
                    ContentionEffect::SettlementDeferred,
                    ContentionFacts::default(),
                );
                Ok(false)
            }
        }
    }

    /// Publishes the retirement effect, and the invalidation that went with it.
    fn report_settlement(&self, state: &LedgerState, settlement: Settlement) {
        if settlement.invalidated_demand() {
            self.emit(
                state,
                ContentionEffect::DemandInvalidated,
                ContentionFacts::default(),
            );
        }
        self.emit(state, settlement.effect(), ContentionFacts::default());
    }

    /// Removes one empty table cell, and its tenant when that was the last table.
    ///
    /// Every category is inspected before anything is removed, so a nonempty
    /// refusal leaves the cell and every derived total exactly as it found them.
    /// A cell that vanished while still holding a balance would strand it: the
    /// pod total would keep counting capacity no owner can return.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::NonEmpty`] naming the first category that
    /// still holds capacity.
    fn retire_if_empty(
        state: &mut LedgerState,
        key: &ContentionKey,
    ) -> Result<Settlement, ContentionRefusal> {
        let Some(tenant) = state.tenants.get_mut(&key.tenant) else {
            return Ok(Settlement::Untracked);
        };
        let Some(cell) = tenant.tables.get(&key.table) else {
            return Ok(Settlement::Untracked);
        };
        for category in ContentionCategory::ALL {
            let held = cell.get(category);
            if held > 0 {
                return Err(ContentionRefusal::NonEmpty {
                    category: category.label(),
                    held,
                });
            }
        }
        tenant.tables.remove(&key.table);
        // A category record belongs to an active table that was refused
        // capacity. The table is gone, so the record is stale, and leaving it
        // would reserve a lifecycle quantum for an owner that is no longer
        // waiting for anything.
        let before = state.demand.len();
        state.demand.retain(|record| &record.key != key);
        let invalidated = before != state.demand.len();
        if tenant.tables.is_empty() {
            state.tenants.remove(&key.tenant);
            return Ok(Settlement::retired(true, invalidated));
        }
        Ok(Settlement::retired(false, invalidated))
    }

    /// Charges `amount` in one category to an active table.
    ///
    /// The ceiling is recomputed here, on every charge, from the live owners and
    /// the live contenders -- never cached and never derived from a configured
    /// cardinality. Three bounds apply in this one category, and nothing is
    /// summed across categories:
    ///
    /// 1. **Contender reservation.** Every distinct contender queued for this
    ///    category *ahead of the caller* holds back one lifecycle quantum. An
    ///    incumbent that has just released capacity is behind all of them, which
    ///    is what makes reacquisition impossible while someone waits; the
    ///    contender at the head of the queue reserves nothing against itself and
    ///    so receives that capacity.
    /// 2. **Tenant level.** Max-min fair share of what is left, over the live
    ///    tenants.
    /// 3. **Table level.** Max-min fair share of the charging tenant's own
    ///    level, over its live tables.
    ///
    /// An owner already above a level it used to be under keeps what it holds;
    /// only this growth is refused. Every refusal registers the caller's own
    /// bounded, expiring, identity-only demand record, so a retry inherits the
    /// queue position this attempt earned.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::Inactive`] when the table holds no cell,
    /// [`ContentionRefusal::Exhausted`] naming the bound the charge ran into,
    /// and [`ContentionRefusal::Poisoned`] when the ledger lock is poisoned.
    pub fn charge(
        &self,
        key: &ContentionKey,
        category: ContentionCategory,
        amount: usize,
    ) -> Result<(), ContentionRefusal> {
        let mut state = self.lock()?;
        self.prune(&mut state);
        self.charge_locked(&mut state, &self.request(key, category, amount))
    }
    /// Reconciles the demand queue with a charge that has already committed.
    ///
    /// Called only after the cell's new balance is written, so every branch here
    /// describes a fact rather than a decision. Borrowing above the lifecycle
    /// quantum is reported because it is idle capacity the owner must expect to
    /// give back; a turnover below the quantum is reported *and leaves the
    /// caller's record in place*, because an incumbent would otherwise reacquire
    /// the remainder the contender was still owed; reaching the quantum retires
    /// the record, which is the only way a table's turn ends successfully.
    fn settle_charge(
        &self,
        state: &mut LedgerState,
        request: &ChargeRequest<'_>,
        table_held: usize,
        charging_table: usize,
    ) {
        let key = request.key;
        let category = request.category;
        let amount = request.amount;
        if charging_table > request.quantum {
            // Above its own lifecycle quantum, so the excess is genuinely idle
            // capacity nobody else is entitled to right now. This is the
            // work-conserving rule doing its job, not an owner overreaching.
            self.emit(
                state,
                ContentionEffect::CapacityBorrowed,
                ContentionFacts {
                    category: Some(category),
                    ceiling: request.quantum,
                    requested: amount,
                    held_before: table_held,
                    held_after: charging_table,
                    ..ContentionFacts::default()
                },
            );
        }
        // The queue is released only once the caller actually holds the whole
        // lifecycle quantum it queued for. A partial turnover is work-conserving
        // -- the caller keeps what it just received -- but it does not end the
        // caller's turn, because an incumbent would otherwise take the rest of
        // the quantum the contender was still owed.
        if charging_table < request.quantum && state.has_demand(key, category) {
            self.emit(
                state,
                ContentionEffect::DemandPartiallyServed,
                ContentionFacts {
                    category: Some(category),
                    ceiling: request.quantum,
                    requested: amount,
                    held_before: table_held,
                    held_after: charging_table,
                    ..ContentionFacts::default()
                },
            );
        }
        if charging_table >= request.quantum {
            state
                .demand
                .retain(|record| &record.key != key || record.category != Some(category));
            self.emit(
                state,
                ContentionEffect::DemandRetired,
                ContentionFacts {
                    category: Some(category),
                    ceiling: request.quantum,
                    requested: amount,
                    held_before: table_held,
                    held_after: charging_table,
                    ..ContentionFacts::default()
                },
            );
        }
    }

    /// Applies one category charge to already-locked ledger state.
    ///
    /// Split out from [`Self::charge`] so [`Self::admit`] can apply both initial
    /// charges inside the same lock as activation. Every bound, refusal, demand
    /// record, and commit is identical on both paths by construction.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::Inactive`] when the table holds no cell and
    /// [`ContentionRefusal::Exhausted`] naming the bound the charge ran into.
    fn charge_locked(
        &self,
        state: &mut LedgerState,
        request: &ChargeRequest<'_>,
    ) -> Result<(), ContentionRefusal> {
        let key = request.key;
        let category = request.category;
        let amount = request.amount;
        let capacity = self.capacity.component(category);
        if !state.is_active(key) {
            return Err(ContentionRefusal::Inactive);
        }

        // Capacity the queue is holding for contenders ahead of this caller.
        let contenders = state.reserved_ahead(key, category, request.quantum);
        let reserved = contenders.saturating_mul(request.quantum).min(capacity);
        let available = capacity - reserved;

        // Hard pod bound first. A fair level says what an owner *should* hold;
        // this says what the pod physically has left once the queue is honoured.
        // The two differ exactly when an incumbent is above its level, and
        // because acknowledged work is never revoked the incumbent keeps it and
        // everyone else waits for it to drain rather than over-committing.
        let pod_committed: usize = state
            .tenants
            .values()
            .map(|tenant| tenant.committed(category))
            .sum();
        if pod_committed.saturating_add(amount) > available {
            return Err(self.refuse(state, request, "pod", available, pod_committed, contenders));
        }

        let (mut tenant_demands, charging_tenant) = state.tenant_demands(request);
        let tenant_level = max_min_level(available, &mut tenant_demands);
        if tenant_level != usize::MAX {
            // Only a level that actually binds is worth publishing. An
            // unconstrained pod recomputes a non-binding level on every charge,
            // and reporting those would bury the recomputation that mattered.
            self.emit(
                state,
                ContentionEffect::ShareRecomputed,
                ContentionFacts {
                    category: Some(category),
                    ceiling: tenant_level,
                    requested: amount,
                    reserved_contenders: contenders,
                    ..ContentionFacts::default()
                },
            );
        }
        if charging_tenant > tenant_level {
            let held = state
                .tenants
                .get(&key.tenant)
                .map_or(0, |cell| cell.committed(category));
            return Err(self.refuse(state, request, "tenant", tenant_level, held, contenders));
        }

        let (mut table_demands, charging_table, table_held) = state.table_demands(request)?;
        let table_level = max_min_level(tenant_level.min(available), &mut table_demands);
        if charging_table > table_level {
            return Err(self.refuse(state, request, "table", table_level, table_held, contenders));
        }

        if let Some(cell) = state
            .tenants
            .get_mut(&key.tenant)
            .and_then(|tenant| tenant.tables.get_mut(&key.table))
        {
            cell.set(category, charging_table);
        }
        self.settle_charge(state, request, table_held, charging_table);
        self.emit(
            state,
            ContentionEffect::ChargeCommitted,
            ContentionFacts {
                category: Some(category),
                ceiling: table_level,
                requested: amount,
                held_before: table_held,
                held_after: charging_table,
                reserved_contenders: contenders,
                ..ContentionFacts::default()
            },
        );
        Ok(())
    }

    /// Records the caller's demand and builds the refusal describing the bound.
    ///
    /// Every refusal path goes through here so that no bound can be added later
    /// which refuses a caller without also queueing it; a caller refused without
    /// a record would lose its place to whoever retries next.
    fn refuse(
        &self,
        state: &mut LedgerState,
        request: &ChargeRequest<'_>,
        scope: &'static str,
        ceiling: usize,
        held: usize,
        contenders: usize,
    ) -> ContentionRefusal {
        self.enqueue_demand(state, request.key, Some(request.category));
        let effect = if contenders > 0 {
            // The bound this caller ran into exists because someone is queued
            // ahead of it. Reported as its own decision so an operator can tell
            // "the pod is full" apart from "a contender is being protected".
            ContentionEffect::IncumbentBlocked
        } else {
            ContentionEffect::ChargeRefused
        };
        self.emit(
            state,
            effect,
            ContentionFacts {
                category: Some(request.category),
                ceiling,
                requested: request.amount,
                held_before: held,
                held_after: held,
                reserved_contenders: contenders,
                ..ContentionFacts::default()
            },
        );
        ContentionRefusal::Exhausted {
            scope,
            category: request.category.label(),
            ceiling,
            held,
            requested: request.amount,
        }
    }

    /// Returns `amount` in one category to an active table's cell.
    ///
    /// Releasing more than the cell holds is an accounting bug, and it fails
    /// closed: the subtraction is checked, the refusal is typed, and nothing at
    /// the table, tenant or pod level moves. Saturating here instead would
    /// silently reconcile the caller's bad arithmetic against the ledger's good
    /// arithmetic and hide the bug that produced it.
    ///
    /// Pod and tenant totals are derived from the cell, so returning it here is
    /// the whole release, and the freed amount is immediately visible to every
    /// other owner's next fair-level computation.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::Inactive`] when the table holds no cell,
    /// [`ContentionRefusal::OverRelease`] when the cell holds less than
    /// `amount`, leaving every total untouched, and
    /// [`ContentionRefusal::Poisoned`] when the ledger lock is poisoned.
    pub fn release(
        &self,
        key: &ContentionKey,
        category: ContentionCategory,
        amount: usize,
    ) -> Result<(), ContentionRefusal> {
        let mut state = self.lock()?;
        let held = state.held(key, category);
        match Self::release_locked(&mut state, key, category, amount) {
            Ok(()) => {
                self.emit(
                    &state,
                    ContentionEffect::ChargeReleased,
                    ContentionFacts {
                        category: Some(category),
                        requested: amount,
                        held_before: held,
                        held_after: held.saturating_sub(amount),
                        ..ContentionFacts::default()
                    },
                );
                Ok(())
            }
            Err(refusal) => {
                if matches!(refusal, ContentionRefusal::OverRelease { .. }) {
                    self.emit(
                        &state,
                        ContentionEffect::OverReleaseRefused,
                        ContentionFacts {
                            category: Some(category),
                            requested: amount,
                            held_before: held,
                            held_after: held,
                            ..ContentionFacts::default()
                        },
                    );
                }
                Err(refusal)
            }
        }
    }

    /// Returns `amount` to one cell under an already-held lock.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::Inactive`] when the table holds no cell and
    /// [`ContentionRefusal::OverRelease`] when the cell holds less than
    /// `amount`, in which case nothing is mutated.
    fn release_locked(
        state: &mut LedgerState,
        key: &ContentionKey,
        category: ContentionCategory,
        amount: usize,
    ) -> Result<(), ContentionRefusal> {
        let cell = state
            .tenants
            .get_mut(&key.tenant)
            .and_then(|tenant| tenant.tables.get_mut(&key.table))
            .ok_or(ContentionRefusal::Inactive)?;
        let held = cell.get(category);
        let next_usage = held
            .checked_sub(amount)
            .ok_or(ContentionRefusal::OverRelease {
                category: category.label(),
                held,
                requested: amount,
            })?;
        cell.set(category, next_usage);
        Ok(())
    }

    /// Returns the number of currently active table cells across every tenant.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::Poisoned`] when the ledger lock is poisoned.
    pub fn active_cells(&self) -> Result<usize, ContentionRefusal> {
        Ok(self.lock()?.active_tables())
    }

    /// Returns the number of currently active tenant cells.
    ///
    /// Exposed alongside [`Self::active_cells`] because both counts are purely
    /// observed: neither is configured, and neither bounds the other.
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
    /// Summed from the live owners, so it is exactly what the pod is holding and
    /// never includes capacity set aside for an identity that never arrived.
    ///
    /// # Errors
    ///
    /// Returns [`ContentionRefusal::Poisoned`] when the ledger lock is poisoned.
    pub fn committed(&self, category: ContentionCategory) -> Result<usize, ContentionRefusal> {
        let state = self.lock()?;
        Ok(state
            .tenants
            .values()
            .map(|tenant| tenant.committed(category))
            .sum())
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

    /// An already-active table keeps its item demand when the byte leg refuses.
    ///
    /// This is the rollback case that a `Created`-only undo misses. The table is
    /// already active, so nothing needs uninstalling, but its item leg still
    /// satisfied and retired a demand record it had earned by waiting. Returning
    /// only the item charge would drop the table to zero ownership *and* to the
    /// back of the queue, which is precisely the position an over-share
    /// incumbent reacquires from. The claim must come back where it was.
    #[test]
    fn a_failed_byte_leg_returns_the_item_demand_the_item_leg_retired() {
        let policy = ScribeArtifactPolicy::default();
        let quantum = policy.reserve_vector().admission_bytes;
        let ledger = ledger_for(2);
        let incumbent = key(1, "incumbent");
        let contender = key(2, "contender");
        let rival = key(3, "rival");

        ledger
            .admit(&incumbent, quantum * 2)
            .expect("a lone owner borrows every idle byte");
        ledger
            .activate(&contender)
            .expect("the pod completes a second table");
        ledger
            .charge(&contender, ContentionCategory::AdmissionItems, 2)
            .expect_err("the pod holds no second item");
        ledger
            .activate(&rival)
            .expect_err("the pod is at its derived ownership ceiling");

        let before = claims_of(&ledger);
        assert_eq!(
            before,
            vec![
                (contender.clone(), Some(ContentionCategory::AdmissionItems)),
                (rival.clone(), None),
            ],
            "the contender waits ahead of the rival before the failed admission"
        );

        ledger
            .admit(&contender, quantum)
            .expect_err("every byte is owned, so the byte leg refuses");

        let after = claims_of(&ledger);
        assert_eq!(
            after[..before.len()],
            before[..],
            "the retired item claim returns to the position it held, still ahead of the rival"
        );
        assert_eq!(
            after[before.len()..],
            [(contender.clone(), Some(ContentionCategory::AdmissionBytes))],
            "the refused byte leg leaves its own claim behind the priority it kept"
        );
        assert_eq!(
            ledger.usage(&contender, ContentionCategory::AdmissionItems),
            Ok(0),
            "the item charge is returned with the claim"
        );
        assert_eq!(
            ledger.active_cells(),
            Ok(2),
            "an already-active table is not uninstalled by a failed charge"
        );
    }

    /// Telemetry totals reconcile against the ledger state that produced them.
    ///
    /// The counters are only worth publishing if they describe the same pod the
    /// ledger describes. Installed minus released vectors must equal the live
    /// cell count, and every opened admission transition must have closed.
    #[test]
    fn telemetry_totals_reconcile_with_the_ledger_they_describe() {
        let policy = ScribeArtifactPolicy::default();
        let quantum = policy.reserve_vector().admission_bytes;
        let ledger = ledger_for(3);
        let first = key(4, "first");
        let second = key(5, "second");

        ledger.admit(&first, quantum).expect("the pod is idle");
        ledger.admit(&second, quantum).expect("the pod has room");
        assert_eq!(ledger.telemetry_totals().live_vectors(), 2);
        assert_eq!(ledger.active_cells(), Ok(2));

        ledger
            .release(&second, ContentionCategory::AdmissionBytes, quantum)
            .expect("the owner returns its bytes");
        ledger
            .release(&second, ContentionCategory::AdmissionItems, 1)
            .expect("the owner returns its item");

        let snapshot = ledger.telemetry_totals();
        assert_eq!(
            snapshot.live_vectors(),
            u64::try_from(ledger.active_cells().expect("the ledger reports its cells"))
                .expect("a pod holds far fewer cells than a u64 counts"),
            "an emptied cell settles in the counters exactly as it does in state"
        );
        assert!(
            snapshot.demand_transitions() > 0,
            "activating a table is itself a demand transition and must be published"
        );
        assert_eq!(
            ledger.waiting_demand(),
            Ok(0),
            "every published transition ended with an empty queue"
        );
        assert_eq!(
            snapshot.starts(),
            snapshot.terminals(),
            "every admission transition the ledger opened was closed"
        );
    }

    /// Reports every waiting claim in queue order, as identity and category.
    ///
    /// Queue order *is* the priority the ledger confers, so a rollback that
    /// preserves a record but moves it has still lost what the record was for.
    /// This reads the live queue rather than any counter derived from it.
    ///
    /// # Panics
    ///
    /// Panics when the ledger lock is poisoned.
    fn claims_of(
        ledger: &ScribeContentionLedger,
    ) -> Vec<(ContentionKey, Option<ContentionCategory>)> {
        let state = ledger.lock().expect("the ledger lock is healthy");
        state
            .demand
            .iter()
            .map(|record| (record.key.clone(), record.category))
            .collect()
    }

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

    /// Builds a ledger whose measured capacity completes exactly `vectors` tables.
    ///
    /// The capacity is stated as a multiple of the lifecycle vector because that
    /// is what a real node's resources reduce to. No tenant or table count is
    /// configured anywhere: `vectors` is a resource quantity, and the ownership
    /// ceiling the ledger derives from it is what the tests then observe.
    ///
    /// # Panics
    ///
    /// Panics when the default policy cannot serve the capacity it derives,
    /// which would be a fixture bug rather than a ledger behavior.
    fn ledger_for(vectors: usize) -> ScribeContentionLedger {
        ledger_with_demand(
            vectors,
            DEFAULT_CONTENTION_DEMAND_CAPACITY,
            DEFAULT_CONTENTION_DEMAND_TTL,
        )
    }

    /// Builds a ledger with explicit demand-queue bounds over `vectors` tables.
    ///
    /// # Panics
    ///
    /// Panics when the derived capacity does not satisfy the default policy.
    fn ledger_with_demand(
        vectors: usize,
        demand_capacity: usize,
        demand_ttl: Duration,
    ) -> ScribeContentionLedger {
        let policy = ScribeArtifactPolicy::default();
        let vector = policy.reserve_vector();
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
            .expect("a fixture capacity must hold one complete lifecycle vector");
        ScribeContentionLedger::with_demand_bounds(policy, capacity, demand_capacity, demand_ttl)
    }

    /// Returns the pod's total capacity in one category for a `vectors` fixture.
    fn capacity_of(vectors: usize, category: ContentionCategory) -> usize {
        ScribeArtifactPolicy::default()
            .reserve_vector()
            .component(category)
            * vectors
    }

    /// Proves a queued contender outranks an incumbent that queued first.
    ///
    /// Extracted from [`tenant_admission_is_elastic_fair_and_balanced`] so that
    /// owner stays inside the repository line bound while still asserting the
    /// clause: once a contender has been refused its demand is queued, and the
    /// capacity the incumbent then releases is held for that contender rather than
    /// being re-taken -- even when the incumbent queued first and asks for exactly
    /// what it dropped. Without it "fair" would only restate "the pod is full" and
    /// a busy incumbent could starve a peer by retrying faster than it.
    ///
    /// `whole` is the pod's whole measured `Active` capacity for the fixture.
    ///
    /// # Panics
    ///
    /// Panics when the incumbent reacquires ahead of the unserved contender, when
    /// the refused reacquisition moves the incumbent's own level, or when the
    /// contender never receives the released capacity.
    fn contended_incumbent_yields_to_a_queued_peer(whole: usize) {
        // Fair, separately from full: once a contender has been refused its
        // demand is queued, and the capacity the incumbent then releases is
        // held for that contender rather than being re-taken -- even when the
        // incumbent queued first and asks for exactly what it dropped. Without
        // this the "fair" clause would only restate "the pod is full" and a
        // busy incumbent could starve a peer by retrying faster than it.
        let quantum = capacity_of(8, ContentionCategory::Active) / 8;
        let contested = ledger_for(8);
        let first = key(1, "hot");
        let second = key(2, "cold");
        contested
            .activate(&first)
            .expect("the first table activates");
        contested
            .charge(&first, ContentionCategory::Active, whole)
            .expect("a lone owner borrows every genuinely idle byte");
        contested
            .activate(&second)
            .expect("the contender activates");
        contested
            .charge(&first, ContentionCategory::Active, quantum)
            .expect_err("a full pod refuses the incumbent and queues it first");
        contested
            .charge(&second, ContentionCategory::Active, quantum)
            .expect_err("a full pod refuses the contender and queues it behind");
        contested
            .release(&first, ContentionCategory::Active, quantum)
            .expect("the incumbent drains through the normal lifecycle route");
        let bound = contested
            .charge(&first, ContentionCategory::Active, quantum)
            .expect_err("a served incumbent may not reacquire ahead of an unserved contender");
        assert!(
            matches!(bound, ContentionRefusal::Exhausted { .. }),
            "reacquisition ahead of a contender is a typed contention refusal; got {bound:?}"
        );
        assert_eq!(
            contested
                .usage(&first, ContentionCategory::Active)
                .expect("usage"),
            whole - quantum,
            "the refused reacquisition moves nothing at the table level"
        );
        contested
            .charge(&second, ContentionCategory::Active, quantum)
            .expect("the waiting contender receives the released capacity");
        assert_eq!(
            contested
                .committed(ContentionCategory::Active)
                .expect("pod"),
            whole,
            "honouring the reservation neither strands nor overcommits capacity"
        );
    }

    /// Proves a complete drain returns every accounting level to zero.
    ///
    /// Extracted from [`tenant_admission_is_elastic_fair_and_balanced`] so that
    /// owner stays inside the repository line bound. Releasing the remainder
    /// each owner still holds must retire both emptied cells and their tenants
    /// and return every category's pod total to zero; a level that kept a
    /// residue would strand an ownership slot no later charge could reclaim.
    ///
    /// # Panics
    ///
    /// Panics when a release is refused, when an emptied cell does not settle,
    /// when cells or tenants remain active, or when any category still reports
    /// committed capacity.
    fn balanced_drain_retires_every_level(
        ledger: &ScribeContentionLedger,
        incumbent: &ContentionKey,
        contender: &ContentionKey,
        whole: usize,
        half: usize,
    ) {
        // Balanced: releasing everything returns each level to zero and retires
        // both emptied cells and their tenants.
        ledger
            .release(incumbent, ContentionCategory::Active, whole - half)
            .expect("release");
        ledger
            .release(contender, ContentionCategory::Active, half)
            .expect("release");
        for owner in [incumbent, contender] {
            assert!(
                ledger.settle(owner).expect("settlement is readable"),
                "an emptied cell must retire rather than strand an ownership slot"
            );
        }
        assert_eq!(ledger.active_cells().expect("cells"), 0);
        assert_eq!(ledger.active_tenants().expect("tenants"), 0);
        for category in ContentionCategory::ALL {
            assert_eq!(
                ledger.committed(category).expect("pod totals"),
                0,
                "{} must drain completely",
                category.label()
            );
        }
    }

    /// AC22/AC26 unit owner: tenant admission is elastic, fair, and balanced.
    ///
    /// One named owner for the three properties the ledger exists to hold
    /// together, because each is only meaningful in the presence of the others:
    /// capacity is lent work-conservingly to whoever is asking (elastic), the
    /// level is recomputed from the live owners the moment a peer contends
    /// (fair), and every success, refusal and release moves the identical
    /// amount at the table, tenant and pod levels or moves it at none
    /// (balanced). Categories are proven independent rather than summed.
    ///
    /// # Panics
    ///
    /// Panics when idle capacity is stranded, when a contender does not shrink
    /// an incumbent's level, when acknowledged ownership is revoked, when a
    /// refusal moves a counter, when one category's exhaustion constrains
    /// another, or when a fully drained ledger still reports ownership.
    #[test]
    fn tenant_admission_is_elastic_fair_and_balanced() {
        let ledger = ledger_for(8);
        let incumbent = key(1, "events");
        let contender = key(2, "events");
        let whole = capacity_of(8, ContentionCategory::Active);

        // Elastic: the only owner asking reaches the pod's whole measured
        // capacity. No share is reserved for a tenant that has sent nothing.
        ledger
            .activate(&incumbent)
            .expect("the first table activates");
        ledger
            .charge(&incumbent, ContentionCategory::Active, whole)
            .expect("a lone owner borrows every genuinely idle byte");
        assert_eq!(
            ledger.committed(ContentionCategory::Active).expect("pod"),
            whole
        );

        // Fair: a second live owner immediately shrinks the incumbent's level
        // for new acquisition, but never revokes what it already holds.
        ledger
            .activate(&contender)
            .expect("the contender activates");
        assert_eq!(
            ledger
                .usage(&incumbent, ContentionCategory::Active)
                .expect("usage"),
            whole,
            "acknowledged ownership is never revoked by a new contender"
        );
        let frozen = ledger
            .charge(&incumbent, ContentionCategory::Active, 1)
            .expect_err("an over-share incumbent may not grow while a peer waits");
        assert!(
            matches!(
                frozen,
                ContentionRefusal::Exhausted {
                    scope: "pod",
                    ceiling,
                    ..
                } if ceiling == whole
            ),
            "an incumbent holding the whole pod runs into the pod bound, got {frozen:?}"
        );
        assert_eq!(
            ledger
                .usage(&incumbent, ContentionCategory::Active)
                .expect("usage"),
            whole,
            "a refusal moves nothing at the table level"
        );
        assert_eq!(
            ledger.committed(ContentionCategory::Active).expect("pod"),
            whole,
            "a refusal moves nothing at the pod level"
        );

        contended_incumbent_yields_to_a_queued_peer(whole);

        // Categories are independent: exhausting active bytes leaves the
        // immutable, stage and scratch levels untouched.
        for category in [
            ContentionCategory::Immutable,
            ContentionCategory::DurableStage,
            ContentionCategory::MergeScratch,
        ] {
            let half = capacity_of(8, category) / 2;
            ledger
                .charge(&contender, category, half)
                .expect("an exhausted category never constrains another");
            ledger
                .release(&contender, category, half)
                .expect("the independent category returns exactly what it took");
        }

        // The next released capacity reaches the waiting contender, and the
        // incumbent's drain is accounted at every level.
        let half = whole / 2;
        ledger
            .release(&incumbent, ContentionCategory::Active, half)
            .expect("acknowledged ownership drains normally");
        assert_eq!(
            ledger
                .tenant_usage(&tenant_id(1), ContentionCategory::Active)
                .expect("tenant usage"),
            whole - half
        );
        ledger
            .charge(&contender, ContentionCategory::Active, half)
            .expect("released capacity is offered to the waiting contender");
        assert_eq!(
            ledger.committed(ContentionCategory::Active).expect("pod"),
            whole,
            "the pod total is exactly the sum of what its owners hold"
        );

        balanced_drain_retires_every_level(&ledger, &incumbent, &contender, whole, half);
    }

    /// One tenant with one table may use every genuinely idle byte in the pod.
    ///
    /// This is the work-conserving rule: capacity is never stranded behind an
    /// identity that has not sent traffic, so a single owner faces no ceiling
    /// below the pod's real capacity, and exactly one unit beyond it is refused.
    ///
    /// # Panics
    ///
    /// Panics when a lone owner is capped below pod capacity, when it is allowed
    /// past it, or when only one tenant cell was not created lazily.
    #[test]
    fn a_single_table_may_use_all_idle_capacity() {
        let ledger = ledger_for(4);
        let only = key(1, "events");
        ledger.activate(&only).expect("the first table activates");
        assert_eq!(ledger.active_tenants().expect("tenants"), 1);
        assert_eq!(ledger.active_cells().expect("cells"), 1);

        let whole = capacity_of(4, ContentionCategory::Active);
        ledger
            .charge(&only, ContentionCategory::Active, whole)
            .expect("a lone owner reaches the pod's whole measured capacity");
        assert_eq!(
            ledger
                .usage(&only, ContentionCategory::Active)
                .expect("usage"),
            whole
        );
        let refusal = ledger
            .charge(&only, ContentionCategory::Active, 1)
            .expect_err("one unit past measured capacity must refuse");
        assert!(matches!(
            refusal,
            ContentionRefusal::Exhausted { scope: "pod", .. }
        ));
    }

    /// One tenant may own every table the measured capacity can complete.
    ///
    /// The ceiling is derived from resources divided by the lifecycle vector,
    /// not from a configured tables-per-tenant width, so one tenant reaches all
    /// of it and the next table is refused on resources rather than identity.
    ///
    /// # Panics
    ///
    /// Panics when a single tenant is capped below the derived ceiling, or when
    /// the refusal past it does not name the derived ceiling.
    #[test]
    fn one_tenant_owns_every_table_the_resources_complete() {
        let ledger = ledger_for(16);
        assert_eq!(ledger.ownership_ceiling(), 16);
        for table in 0..16 {
            ledger
                .activate(&key(1, &format!("table_{table}")))
                .expect("every table the resources complete activates");
        }
        assert_eq!(ledger.active_cells().expect("cells"), 16);

        let refusal = ledger
            .activate(&key(1, "table_16"))
            .expect_err("the seventeenth table exceeds the measured resources");
        assert!(matches!(
            refusal,
            ContentionRefusal::OwnershipCeiling {
                ceiling: 16,
                active: 16,
                waiting: 0,
            }
        ));
    }

    /// Ten hungry tenants converge on an equal max-min share of one category.
    ///
    /// The share is never a configured division: it is recomputed from the live
    /// owners, which is why nine idle tenants leave the tenth unconstrained in
    /// the previous test and ten hungry ones divide the category here.
    ///
    /// # Panics
    ///
    /// Panics when an equal share is refused, when a tenant is allowed past it
    /// while its peers are hungry, or when the categories were summed.
    #[test]
    fn ten_hungry_tenants_receive_equal_max_min_shares() {
        const TENANTS: usize = 10;
        let ledger = ledger_for(16);
        for tenant in 0..TENANTS {
            let owner = key(u8::try_from(tenant).expect("tenant ordinal"), "events");
            ledger.activate(&owner).expect("each tenant activates");
        }
        let whole = capacity_of(16, ContentionCategory::Active);
        let share = whole / TENANTS;
        assert!(
            whole - share * TENANTS < TENANTS,
            "an equal division leaves less than one unit per tenant unallocated"
        );
        for tenant in 0..TENANTS {
            let owner = key(u8::try_from(tenant).expect("tenant ordinal"), "events");
            ledger
                .charge(&owner, ContentionCategory::Active, share)
                .expect("an equal share is always available to a live owner");
        }
        for tenant in 0..TENANTS {
            let owner = key(u8::try_from(tenant).expect("tenant ordinal"), "events");
            let refusal = ledger
                .charge(&owner, ContentionCategory::Active, share)
                .expect_err("no tenant may exceed its share while its peers are hungry");
            assert!(matches!(refusal, ContentionRefusal::Exhausted { .. }));
            // A different category is untouched by exhaustion in this one.
            ledger
                .charge(&owner, ContentionCategory::Immutable, share)
                .expect("byte categories are governed independently");
        }
    }

    /// A hot incumbent keeps what it holds and is refused only new growth.
    ///
    /// Acknowledged work is never revoked. When a contender arrives and shrinks
    /// every level, the incumbent's committed bytes stay exactly where they are
    /// and drain through the normal lifecycle; only its next acquisition fails.
    ///
    /// # Panics
    ///
    /// Panics when the incumbent's committed bytes change on contention, when it
    /// is allowed to grow anyway, or when the contender never gets the capacity
    /// the incumbent released.
    #[test]
    fn a_hot_incumbent_drains_rather_than_being_revoked() {
        let ledger = ledger_for(4);
        let incumbent = key(1, "hot");
        let contender = key(2, "cold");
        let whole = capacity_of(4, ContentionCategory::Active);
        ledger.activate(&incumbent).expect("incumbent activates");
        ledger
            .charge(&incumbent, ContentionCategory::Active, whole)
            .expect("an idle pod lets the first owner take everything");

        ledger.activate(&contender).expect("contender activates");
        assert_eq!(
            ledger
                .usage(&incumbent, ContentionCategory::Active)
                .expect("usage"),
            whole,
            "a new contender must never revoke acknowledged work"
        );
        ledger
            .charge(&incumbent, ContentionCategory::Active, 1)
            .expect_err("an over-share incumbent may not acquire more while a peer waits");
        ledger
            .charge(&contender, ContentionCategory::Active, 1)
            .expect_err("the pod is full until the incumbent drains");

        ledger
            .release(&incumbent, ContentionCategory::Active, whole / 2)
            .expect("released through the normal lifecycle route");
        ledger
            .charge(&contender, ContentionCategory::Active, whole / 2)
            .expect("released capacity reaches the waiting contender");
    }

    /// Demand records are identity-only, bounded, expiring, and ordering-fair.
    ///
    /// A refused contender is served before the incumbent that released the
    /// capacity can take it back; the queue never grows past its configured
    /// operational bound; and an expired record confers nothing.
    ///
    /// # Panics
    ///
    /// Panics when an incumbent reborrows ahead of a waiting contender, when the
    /// queue grows past its bound, or when an expired record still counts.
    #[test]
    fn contention_demand_is_bounded_expiring_and_ordering_fair() {
        let ledger = ledger_with_demand(1, 1, Duration::from_secs(30));
        let incumbent = key(1, "hot");
        let contender = key(2, "cold");
        let third = key(3, "later");
        ledger.activate(&incumbent).expect("incumbent activates");

        ledger
            .activate(&contender)
            .expect_err("a full pod refuses the contender and records its demand");
        assert_eq!(ledger.waiting_demand().expect("demand"), 1);
        ledger
            .activate(&third)
            .expect_err("a full pod refuses every further contender");
        assert_eq!(
            ledger.waiting_demand().expect("demand"),
            1,
            "the demand queue never grows past its configured operational bound"
        );

        ledger
            .deactivate(&incumbent)
            .expect("the incumbent finishes and releases its table");
        ledger
            .activate(&key(1, "hot_again"))
            .expect_err("an incumbent may not reborrow ahead of a waiting contender");
        ledger
            .activate(&contender)
            .expect("the waiting contender receives the next released vector");
        assert_eq!(
            ledger.waiting_demand().expect("demand"),
            0,
            "admission removes the record it was waiting on"
        );

        // An expired record confers no ordering priority at all.
        let expiring = ledger_with_demand(1, 4, Duration::ZERO);
        expiring.activate(&incumbent).expect("incumbent activates");
        expiring
            .activate(&contender)
            .expect_err("a full pod refuses the contender");
        assert_eq!(expiring.waiting_demand().expect("demand"), 0);

        // Cancellation removes it just as admission does.
        let cancelling = ledger_with_demand(1, 4, Duration::from_secs(30));
        cancelling
            .activate(&incumbent)
            .expect("incumbent activates");
        cancelling
            .activate(&contender)
            .expect_err("a full pod refuses the contender");
        cancelling.cancel_demand(&contender).expect("cancel");
        assert_eq!(cancelling.waiting_demand().expect("demand"), 0);
    }

    /// Charging and releasing one category balances back to zero exactly.
    ///
    /// Table, tenant and pod totals are all derived from the same cells, so a
    /// complete release must leave nothing behind at any level, and deactivating
    /// the last table must leave no tenant cell holding capacity.
    ///
    /// # Panics
    ///
    /// Panics when any level fails to return to zero, or when a released tenant
    /// cell survives its last table.
    #[test]
    fn accounting_balances_across_table_tenant_and_pod() {
        let ledger = ledger_for(4);
        let first = key(1, "events");
        let second = key(1, "wyrd_system_events");
        ledger.activate(&first).expect("first table activates");
        ledger.activate(&second).expect("second table activates");

        let amount = capacity_of(4, ContentionCategory::Active) / 4;
        ledger
            .charge(&first, ContentionCategory::Active, amount)
            .expect("charge");
        ledger
            .charge(&second, ContentionCategory::Active, amount)
            .expect("charge");
        assert_eq!(
            ledger
                .tenant_usage(&tenant_id(1), ContentionCategory::Active)
                .expect("tenant usage"),
            amount * 2
        );
        assert_eq!(
            ledger.committed(ContentionCategory::Active).expect("pod"),
            amount * 2
        );

        ledger
            .release(&first, ContentionCategory::Active, amount)
            .expect("release");
        ledger
            .release(&second, ContentionCategory::Active, amount)
            .expect("release");
        assert_eq!(
            ledger.committed(ContentionCategory::Active).expect("pod"),
            0
        );

        ledger.deactivate(&first).expect("deactivate");
        ledger.deactivate(&second).expect("deactivate");
        assert_eq!(ledger.active_tenants().expect("tenants"), 0);
        assert_eq!(
            ledger.committed(ContentionCategory::Active).expect("pod"),
            0
        );
    }

    /// A system table and a dynamic table are governed identically.
    ///
    /// Neither name has a reserved cell and neither has priority: the two
    /// siblings divide their tenant's level equally, and the system table is
    /// refused past it on exactly the same terms as the dynamic one.
    ///
    /// # Panics
    ///
    /// Panics when either table is given a different level from its sibling.
    #[test]
    fn system_and_dynamic_tables_are_governed_identically() {
        let ledger = ledger_for(4);
        let system = key(1, "wyrd_system_events");
        let dynamic = key(1, "orders");
        ledger.activate(&system).expect("system table activates");
        ledger.activate(&dynamic).expect("dynamic table activates");

        let half = capacity_of(4, ContentionCategory::Active) / 2;
        ledger
            .charge(&system, ContentionCategory::Active, half)
            .expect("the system table reaches its equal share");
        ledger
            .charge(&dynamic, ContentionCategory::Active, half)
            .expect("the dynamic table reaches the same equal share");
        for table in [&system, &dynamic] {
            let refusal = ledger
                .charge(table, ContentionCategory::Active, 1)
                .expect_err("neither sibling may exceed the level the other shares");
            assert!(matches!(refusal, ContentionRefusal::Exhausted { .. }));
        }
    }

    /// Reactivating an already-active table changes nothing.
    ///
    /// # Panics
    ///
    /// Panics when repeated activation installs a second cell or disturbs usage.
    #[test]
    fn repeated_activation_is_idempotent() {
        let ledger = ledger_for(4);
        let only = key(1, "events");
        ledger.activate(&only).expect("activate");
        ledger
            .charge(&only, ContentionCategory::Active, 1)
            .expect("charge");
        ledger.activate(&only).expect("re-activate");
        assert_eq!(ledger.active_cells().expect("cells"), 1);
        assert_eq!(
            ledger
                .usage(&only, ContentionCategory::Active)
                .expect("usage"),
            1
        );
    }

    /// A refused first charge leaves no cell behind for a brand new table.
    ///
    /// Activation and the first admission charges are one transition. A cell
    /// that survived a refused first request would hold one of the pod's
    /// derived ownership slots forever while serving nothing.
    ///
    /// # Panics
    ///
    /// Panics when the rolled-back table or its tenant still holds a cell, or
    /// when the pre-existing owner loses anything.
    #[test]
    fn a_refused_first_charge_leaves_no_new_cell() {
        let ledger = ledger_for(4);
        let incumbent = key(1, "hot");
        let arrival = key(2, "cold");
        let whole = capacity_of(4, ContentionCategory::Active);
        ledger.activate(&incumbent).expect("incumbent activates");
        ledger
            .charge(&incumbent, ContentionCategory::Active, whole)
            .expect("the idle pod lets the first owner take everything");

        assert_eq!(
            ledger
                .activate(&arrival)
                .expect("the pod can still carry it"),
            ActivationOutcome::Created
        );
        ledger
            .charge(&arrival, ContentionCategory::Active, 1)
            .expect_err("a full pod refuses the arrival's first charge");
        // The admission path rolls the created cell back on that refusal.
        ledger
            .deactivate(&arrival)
            .expect("an empty cell rolls back");

        assert_eq!(ledger.active_cells().expect("cells"), 1);
        assert_eq!(ledger.active_tenants().expect("tenants"), 1);
        assert_eq!(
            ledger
                .usage(&incumbent, ContentionCategory::Active)
                .expect("usage"),
            whole,
            "rolling back an arrival must not touch a pre-existing owner"
        );
    }

    /// Deactivation is refused while a table still holds capacity.
    ///
    /// # Panics
    ///
    /// Panics when a non-empty cell is removed, when the refusal does not name
    /// the category still held, or when the refusal mutates anything.
    #[test]
    fn deactivating_a_nonempty_table_is_refused_without_mutation() {
        let ledger = ledger_for(4);
        let owner = key(1, "events");
        ledger.activate(&owner).expect("activate");
        ledger
            .charge(&owner, ContentionCategory::Active, 64)
            .expect("charge");

        let refusal = ledger
            .deactivate(&owner)
            .expect_err("a table holding capacity must not vanish");
        assert!(matches!(
            refusal,
            ContentionRefusal::NonEmpty {
                category: "active",
                held: 64,
            }
        ));
        assert_eq!(ledger.active_cells().expect("cells"), 1);
        assert_eq!(
            ledger.committed(ContentionCategory::Active).expect("pod"),
            64
        );

        ledger
            .release(&owner, ContentionCategory::Active, 64)
            .expect("release");
        ledger
            .deactivate(&owner)
            .expect("an empty table deactivates");
        assert_eq!(ledger.active_tenants().expect("tenants"), 0);
    }

    /// Over-release fails closed and moves no counter at any level.
    ///
    /// # Panics
    ///
    /// Panics when a release larger than the cell holds is accepted, or when the
    /// refusal changes table, tenant or pod accounting.
    #[test]
    fn over_release_is_refused_without_mutation() {
        let ledger = ledger_for(4);
        let owner = key(1, "events");
        ledger.activate(&owner).expect("activate");
        ledger
            .charge(&owner, ContentionCategory::Active, 100)
            .expect("charge");

        let refusal = ledger
            .release(&owner, ContentionCategory::Active, 101)
            .expect_err("returning more than the cell holds is an accounting bug");
        assert!(matches!(
            refusal,
            ContentionRefusal::OverRelease {
                category: "active",
                held: 100,
                requested: 101,
            }
        ));
        assert_eq!(
            ledger
                .usage(&owner, ContentionCategory::Active)
                .expect("usage"),
            100
        );
        assert_eq!(
            ledger
                .tenant_usage(&tenant_id(1), ContentionCategory::Active)
                .expect("tenant"),
            100
        );
        assert_eq!(
            ledger.committed(ContentionCategory::Active).expect("pod"),
            100
        );
    }

    /// A refused charge by an already-active table keeps its cell and ownership.
    ///
    /// The refusal says nothing about the work the table is already carrying, so
    /// nothing it owns may be revoked or rolled back.
    ///
    /// # Panics
    ///
    /// Panics when the refusal disturbs the table's cell or its committed bytes.
    #[test]
    fn a_refused_charge_preserves_an_active_table() {
        let ledger = ledger_for(4);
        let owner = key(1, "events");
        let whole = capacity_of(4, ContentionCategory::Active);
        ledger.activate(&owner).expect("activate");
        ledger
            .charge(&owner, ContentionCategory::Active, whole)
            .expect("charge");

        ledger
            .charge(&owner, ContentionCategory::Active, 1)
            .expect_err("one unit past measured capacity refuses");
        assert_eq!(ledger.active_cells().expect("cells"), 1);
        assert_eq!(
            ledger
                .usage(&owner, ContentionCategory::Active)
                .expect("usage"),
            whole
        );
        // Its own demand record does not lock it out of what it already holds.
        ledger
            .release(&owner, ContentionCategory::Active, whole)
            .expect("release");
        ledger
            .charge(&owner, ContentionCategory::Active, whole)
            .expect("an unblocked owner reacquires when nobody else waits");
    }

    /// The last table activates once a vector turns over, and cannot be jumped.
    ///
    /// # Panics
    ///
    /// Panics when the queued contender loses its place to an incumbent's new
    /// table or to a tenant that arrived after it.
    #[test]
    fn a_queued_table_activates_on_turnover_and_cannot_be_bypassed() {
        let ledger = ledger_for(16);
        for table in 0..16 {
            ledger
                .activate(&key(1, &format!("table_{table}")))
                .expect("every table the resources complete activates");
        }
        let queued = key(1, "table_16");
        ledger
            .activate(&queued)
            .expect_err("the seventeenth table waits for a vector to turn over");

        ledger
            .deactivate(&key(1, "table_0"))
            .expect("an empty table turns its vector over");

        ledger
            .activate(&key(1, "incumbent_extra"))
            .expect_err("an incumbent tenant may not jump the queue with a new table");
        ledger
            .activate(&key(2, "new_arrival"))
            .expect_err("a newly arriving tenant may not jump the queue either");
        ledger
            .activate(&queued)
            .expect("the queued table takes the vector that turned over");
        assert_eq!(ledger.active_cells().expect("cells"), 16);
    }

    /// An initial borrower is throttled as real contenders arrive.
    ///
    /// Every continuously retrying tenant makes progress: the borrower drains,
    /// each contender is served in the order it queued, and the borrower cannot
    /// reacquire while any of them is still unserved.
    ///
    /// # Panics
    ///
    /// Panics when a contender is starved, when the borrower reacquires ahead of
    /// one, or when a served contender keeps holding the queue.
    #[test]
    fn an_initial_borrower_is_throttled_as_contenders_arrive() {
        const TENANTS: u8 = 10;
        let ledger = ledger_for(16);
        let borrower = key(0, "greedy");
        let whole = capacity_of(16, ContentionCategory::Active);
        let quantum = capacity_of(1, ContentionCategory::Active);
        ledger.activate(&borrower).expect("borrower activates");
        ledger
            .charge(&borrower, ContentionCategory::Active, whole)
            .expect("an idle pod lets the first owner take everything");

        let contenders: Vec<ContentionKey> =
            (1..TENANTS).map(|tenant| key(tenant, "events")).collect();
        for contender in &contenders {
            ledger
                .activate(contender)
                .expect("each contender activates");
            ledger
                .charge(contender, ContentionCategory::Active, quantum)
                .expect_err("a saturated pod refuses every contender");
        }

        for contender in &contenders {
            ledger
                .release(&borrower, ContentionCategory::Active, quantum)
                .expect("the borrower drains one quantum");
            ledger
                .charge(&borrower, ContentionCategory::Active, quantum)
                .expect_err("the borrower may not reacquire while contenders wait");
            ledger
                .charge(contender, ContentionCategory::Active, quantum)
                .expect("each retrying contender progresses in queue order");
            assert_eq!(
                ledger
                    .usage(contender, ContentionCategory::Active)
                    .expect("usage"),
                quantum
            );
        }
        assert_eq!(
            ledger.committed(ContentionCategory::Active).expect("pod"),
            whole,
            "every quantum the borrower released reached a contender"
        );
    }

    /// Activation and the first admission charges commit or roll back together.
    ///
    /// Reproduces SCRIBE-CAP-01 without a timing loop. A contender that is
    /// installed but not yet charged holds nothing and, if its ordering record
    /// were retired at activation, would read as zero demand: an incumbent
    /// above its recomputed share could take the capacity the contender had
    /// just been promised. Because activation, both charges, demand retirement,
    /// and rollback happen inside one lock, there is no state in which the
    /// contender is an unprotected zero-demand owner, and the incumbent is
    /// refused at exactly the boundary that used to be unlocked.
    ///
    /// # Panics
    ///
    /// Panics when a refused initial transition leaves a cell or loses the
    /// contender's ordering claim, when the incumbent reacquires the released
    /// vector, or when the contender's admitted charges do not both land.
    #[test]
    fn activation_and_initial_charges_are_one_transition() {
        let ledger = ledger_for(2);
        let quantum = ledger.reserve_vector().admission_bytes;
        let incumbent = key(1, "hot");
        let contender = key(2, "cold");

        ledger
            .admit(
                &incumbent,
                capacity_of(2, ContentionCategory::AdmissionBytes),
            )
            .expect("a lone owner may borrow every idle byte");

        // The contender's byte charge cannot fit, so the whole transition must
        // unwind: no cell, no charges, and a live ordering claim.
        let refusal = ledger
            .admit(&contender, quantum)
            .expect_err("the pod has no bytes left for a second table");
        assert!(matches!(refusal, ContentionRefusal::Exhausted { .. }));
        assert_eq!(
            ledger.usage(&contender, ContentionCategory::AdmissionItems),
            Err(ContentionRefusal::Inactive)
        );
        assert_eq!(ledger.active_cells().expect("readable"), 1);
        assert_eq!(
            ledger
                .usage(&incumbent, ContentionCategory::AdmissionItems)
                .expect("the incumbent keeps acknowledged work"),
            1
        );
        assert_eq!(ledger.waiting_demand().expect("readable"), 1);

        // One complete vector turns over. It belongs to the contender, and the
        // incumbent is refused at the exact boundary that used to be unlocked.
        ledger
            .release(&incumbent, ContentionCategory::AdmissionBytes, quantum)
            .expect("the incumbent drains normally");
        assert!(
            ledger
                .charge(&incumbent, ContentionCategory::AdmissionBytes, quantum)
                .is_err(),
            "an over-share incumbent must not reborrow past a waiting contender"
        );

        ledger
            .admit(&contender, quantum)
            .expect("the queued contender receives the released vector");
        assert_eq!(
            ledger
                .usage(&contender, ContentionCategory::AdmissionItems)
                .expect("the contender is active"),
            1
        );
        assert_eq!(
            ledger
                .usage(&contender, ContentionCategory::AdmissionBytes)
                .expect("the contender is active"),
            quantum
        );
    }

    /// A partial turnover is kept but does not end the contender's turn.
    ///
    /// Reproduces SCRIBE-CAP-02. A contender queued for one lifecycle quantum
    /// that receives half of it is still below the quantum it was promised.
    /// Retiring its record on that partial success is what would let the
    /// incumbent reborrow the remainder, so the record survives until the whole
    /// quantum is held.
    ///
    /// # Panics
    ///
    /// Panics when a partial charge retires the contender's claim, when the
    /// incumbent reacquires the remainder, or when the completed quantum does
    /// not retire the claim.
    #[test]
    fn a_partial_charge_keeps_the_contenders_place_in_line() {
        let ledger = ledger_for(2);
        let quantum = ledger.reserve_vector().admission_bytes;
        let half = quantum / 2;
        let incumbent = key(1, "hot");
        let contender = key(2, "cold");

        ledger
            .admit(
                &incumbent,
                capacity_of(2, ContentionCategory::AdmissionBytes),
            )
            .expect("a lone owner may borrow every idle byte");
        ledger
            .activate(&contender)
            .expect("the pod still completes a second table");
        ledger
            .charge(&contender, ContentionCategory::AdmissionBytes, quantum)
            .expect_err("every byte is currently owned");
        assert_eq!(ledger.waiting_demand().expect("readable"), 1);

        ledger
            .release(&incumbent, ContentionCategory::AdmissionBytes, half)
            .expect("the incumbent drains part of one vector");
        ledger
            .charge(&contender, ContentionCategory::AdmissionBytes, half)
            .expect("work-conserving: the contender keeps what turned over");
        assert_eq!(
            ledger.waiting_demand().expect("readable"),
            1,
            "a partial turnover must not forfeit the rest of the contender's turn"
        );
        assert!(
            ledger
                .charge(&incumbent, ContentionCategory::AdmissionBytes, 1)
                .is_err(),
            "the incumbent must not reborrow the remainder the contender is owed"
        );
        // The incumbent's own refusal queued it; it is not the subject here.
        ledger
            .cancel_demand(&incumbent)
            .expect("the incumbent withdraws its record");
        assert_eq!(ledger.waiting_demand().expect("readable"), 1);

        ledger
            .release(&incumbent, ContentionCategory::AdmissionBytes, half)
            .expect("the incumbent drains the rest of the vector");
        ledger
            .charge(&contender, ContentionCategory::AdmissionBytes, half)
            .expect("the contender completes its quantum");
        assert_eq!(
            ledger.waiting_demand().expect("readable"),
            0,
            "a served contender stops holding the queue"
        );
    }

    /// Terminal settlement retires an empty table exactly once, and never a full one.
    ///
    /// Settlement is evaluated after every release, so most calls legitimately
    /// find work still in flight. Those must leave the cell and every derived
    /// total untouched; only the final zero transition removes the table, and
    /// the tenant goes with its last table.
    ///
    /// # Panics
    ///
    /// Panics when a nonempty cell is retired, when a settled cell is retired
    /// twice, or when the tenant outlives its final table.
    #[test]
    fn terminal_settlement_retires_only_a_completely_empty_cell() {
        let ledger = ledger_for(4);
        let first = key(1, "first");
        let second = key(1, "second");
        ledger.admit(&first, 1).expect("first table admits");
        ledger.admit(&second, 1).expect("second table admits");

        for category in [
            ContentionCategory::AdmissionItems,
            ContentionCategory::AdmissionBytes,
        ] {
            assert!(
                !ledger.settle(&first).expect("readable"),
                "{} still holds capacity",
                category.label()
            );
            assert_eq!(ledger.active_cells().expect("readable"), 2);
        }

        ledger
            .release(&first, ContentionCategory::AdmissionItems, 1)
            .expect("items return");
        ledger
            .release(&first, ContentionCategory::AdmissionBytes, 1)
            .expect("bytes return");
        assert!(
            ledger.settle(&first).expect("readable"),
            "the cell is empty"
        );
        assert!(
            !ledger.settle(&first).expect("readable"),
            "an already-settled table is not retired twice"
        );
        assert_eq!(ledger.active_cells().expect("readable"), 1);
        assert_eq!(ledger.active_tenants().expect("readable"), 1);

        ledger
            .release(&second, ContentionCategory::AdmissionItems, 1)
            .expect("items return");
        ledger
            .release(&second, ContentionCategory::AdmissionBytes, 1)
            .expect("bytes return");
        assert!(
            ledger.settle(&second).expect("readable"),
            "the cell is empty"
        );
        assert_eq!(ledger.active_cells().expect("readable"), 0);
        assert_eq!(
            ledger.active_tenants().expect("readable"),
            0,
            "a tenant leaves no cell behind its final table"
        );
    }

    /// Progressive filling satisfies small demands before levelling the rest.
    ///
    /// # Panics
    ///
    /// Panics when an unconstrained set is given a binding level, or when the
    /// level is not the equal division of what the satisfied owners left behind.
    #[test]
    fn max_min_level_fills_progressively() {
        assert_eq!(max_min_level(100, &mut [10, 10]), usize::MAX);
        assert_eq!(max_min_level(100, &mut [10, 100, 100]), 45);
        assert_eq!(max_min_level(100, &mut []), usize::MAX);
    }
}
