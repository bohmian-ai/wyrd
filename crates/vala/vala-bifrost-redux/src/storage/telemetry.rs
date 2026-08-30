//! Production observation for the node's one Bifrost storage owner.
//!
//! Every cache decision and metadata load the storage owner performs is
//! published here and nowhere else. The facade follows the `ScribeTelemetry`
//! pattern: one `record` operation advances the reconcilable totals, emits the
//! closed metric families, and emits the structured event, so a published
//! metric always corresponds to a real state transition rather than to a
//! counter a test incremented on its own.
//!
//! The retained [`MetadataCacheSnapshot`] is the read-only reconciliation
//! surface. It carries enough exact state — starts against each terminal
//! outcome, resident entries and bytes, in-flight loads, and waiters — that a
//! drained node can be proven settled instead of merely quiet.
//!
//! Tenant, table, path, checksum, snapshot, query, and request identities are
//! never metric labels. Scrubbed identity belongs on the caller's own span.

use std::sync::Mutex;
use std::time::Duration;

use num_traits::ToPrimitive as _;

/// What the metadata cache did with one lookup.
///
/// The five members are the complete decision vocabulary. `Bypass` is the only
/// one that carries a non-`None` reason, because it is the only decision taken
/// for a cause outside the lookup itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheEffect {
    /// A resident entry satisfied the lookup.
    Hit,
    /// No resident entry and no in-flight load; this caller owns the load.
    Miss,
    /// An in-flight load already owned this key; this caller joined it.
    Join,
    /// The cache did not participate; the caller loads without retention.
    Bypass,
    /// One resident entry was removed to keep the byte ceiling.
    Evict,
}

impl CacheEffect {
    /// Complete closed inventory, in emission-label order.
    pub const ALL: [Self; 5] = [Self::Hit, Self::Miss, Self::Join, Self::Bypass, Self::Evict];

    /// Returns this effect's stable index into the retained totals.
    const fn index(self) -> usize {
        match self {
            Self::Hit => 0,
            Self::Miss => 1,
            Self::Join => 2,
            Self::Bypass => 3,
            Self::Evict => 4,
        }
    }

    /// Returns the emitted `effect` label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hit => "hit",
            Self::Miss => "miss",
            Self::Join => "join",
            Self::Bypass => "bypass",
            Self::Evict => "evict",
        }
    }
}

/// Why a cache effect was taken, when the effect alone does not say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheEffectReason {
    /// The effect follows from the lookup itself.
    None,
    /// This composition booted with no metadata cache at all.
    Disabled,
    /// The metadata is larger than the whole configured budget.
    Oversized,
    /// The owner is closing and admits no new retention.
    Closing,
}

impl CacheEffectReason {
    /// Complete closed inventory, in emission-label order.
    pub const ALL: [Self; 4] = [Self::None, Self::Disabled, Self::Oversized, Self::Closing];

    /// Returns this reason's stable index into the retained totals.
    const fn index(self) -> usize {
        match self {
            Self::None => 0,
            Self::Disabled => 1,
            Self::Oversized => 2,
            Self::Closing => 3,
        }
    }

    /// Returns the emitted `reason` label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Disabled => "disabled",
            Self::Oversized => "oversized",
            Self::Closing => "closing",
        }
    }
}

/// How one decoded-metadata load ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataLoadOutcome {
    /// Decoded metadata was published to the owner and every waiter.
    Success,
    /// The backend or decoder failed; a closed error was published.
    Failed,
    /// The caller or the owner cancelled before a terminal result.
    Cancelled,
    /// The query or policy deadline elapsed before a terminal result.
    Deadline,
}

impl MetadataLoadOutcome {
    /// Complete closed inventory, in emission-label order.
    pub const ALL: [Self; 4] = [Self::Success, Self::Failed, Self::Cancelled, Self::Deadline];

    /// Returns this outcome's stable index into the retained totals.
    const fn index(self) -> usize {
        match self {
            Self::Success => 0,
            Self::Failed => 1,
            Self::Cancelled => 2,
            Self::Deadline => 3,
        }
    }

    /// Returns the emitted `outcome` label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Deadline => "deadline",
        }
    }
}

/// The storage owner's lifecycle state, as retained for reconciliation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StorageLifecycle {
    /// Accepting new loads.
    #[default]
    Open,
    /// Admitting no new loads while outstanding loaders settle.
    Closing,
    /// Fully settled; no entries, loads, waiters, or tasks remain.
    Closed,
}

impl StorageLifecycle {
    /// Returns the retained lifecycle label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closing => "closing",
            Self::Closed => "closed",
        }
    }
}

/// Exact retained state a caller can reconcile the published metrics against.
///
/// Every field is a total or a live count, never a rate, so a drained node's
/// settled state is a set of equalities rather than a judgement call:
/// [`Self::load_starts`] equals [`Self::load_terminals`], and every live count
/// is zero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MetadataCacheSnapshot {
    /// Decisions taken, indexed by `CacheEffect::index`.
    effects: [u64; CacheEffect::ALL.len()],
    /// Decision reasons taken, indexed by `CacheEffectReason::index`.
    reasons: [u64; CacheEffectReason::ALL.len()],
    /// Metadata loads started by an owning caller.
    load_starts: u64,
    /// Terminal load outcomes, indexed by `MetadataLoadOutcome::index`.
    load_terminals: [u64; MetadataLoadOutcome::ALL.len()],
    /// Successful entries currently retained.
    resident_entries: u64,
    /// Charged bytes currently retained.
    resident_bytes: u64,
    /// Loads with an owner that has not published a terminal result.
    inflight_loads: u64,
    /// Callers joined to another caller's in-flight load.
    waiters: u64,
    /// The owner's retained lifecycle state.
    lifecycle: StorageLifecycle,
    /// Settlements that had no matching admission, indexed by
    /// `TelemetryTransition::index`.
    ///
    /// Always zero on a correct owner. A nonzero value means some path
    /// reconciled twice or reconciled work it never admitted, which is exactly
    /// the condition that would otherwise let a saturating gauge report a
    /// truthful-looking zero over broken accounting.
    anomalies: [u64; TelemetryTransition::ALL.len()],
}

/// One reconcilable live-count transition the owner can settle.
///
/// A closed, deliberately tiny domain: it labels the anomaly counter, so it
/// must never carry a tenant, object, or any other unbounded value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryTransition {
    /// A terminal load settling the in-flight gauge.
    LoadTerminal,
    /// A waiter leaving the waiter gauge.
    WaiterSettled,
}

impl TelemetryTransition {
    /// Every transition, in index order.
    pub(crate) const ALL: [Self; 2] = [Self::LoadTerminal, Self::WaiterSettled];

    /// Returns this transition's dense index into the anomaly totals.
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::LoadTerminal => 0,
            Self::WaiterSettled => 1,
        }
    }

    /// Returns the stable metric label for this transition.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::LoadTerminal => "load_terminal",
            Self::WaiterSettled => "waiter_settled",
        }
    }
}

impl MetadataCacheSnapshot {
    /// Returns how many times one decision was taken.
    #[must_use]
    pub const fn effect(&self, effect: CacheEffect) -> u64 {
        self.effects[effect.index()]
    }

    /// Returns how many times one decision reason was recorded.
    #[must_use]
    pub const fn reason(&self, reason: CacheEffectReason) -> u64 {
        self.reasons[reason.index()]
    }

    /// Returns metadata loads started by an owning caller.
    #[must_use]
    pub const fn load_starts(&self) -> u64 {
        self.load_starts
    }

    /// Returns terminal loads recorded with one outcome.
    #[must_use]
    pub const fn load_terminal(&self, outcome: MetadataLoadOutcome) -> u64 {
        self.load_terminals[outcome.index()]
    }

    /// Returns every terminal load outcome summed.
    ///
    /// A settled owner has this equal to [`Self::load_starts`]; any difference
    /// names loads still outstanding, never an accounting leak on its own.
    #[must_use]
    pub const fn load_terminals(&self) -> u64 {
        let mut total = 0;
        let mut index = 0;
        while index < self.load_terminals.len() {
            total += self.load_terminals[index];
            index += 1;
        }
        total
    }

    /// Returns successful entries currently retained.
    #[must_use]
    pub const fn resident_entries(&self) -> u64 {
        self.resident_entries
    }

    /// Returns charged bytes currently retained.
    #[must_use]
    pub const fn resident_bytes(&self) -> u64 {
        self.resident_bytes
    }

    /// Returns loads whose owner has not yet published a terminal result.
    #[must_use]
    pub const fn inflight_loads(&self) -> u64 {
        self.inflight_loads
    }

    /// Returns callers currently joined to another caller's load.
    #[must_use]
    pub const fn waiters(&self) -> u64 {
        self.waiters
    }

    /// Returns the owner's retained lifecycle state.
    #[must_use]
    pub const fn lifecycle(&self) -> StorageLifecycle {
        self.lifecycle
    }

    /// Returns how many unmatched settlements one transition recorded.
    #[must_use]
    pub const fn anomaly(&self, transition: TelemetryTransition) -> u64 {
        self.anomalies[transition.index()]
    }

    /// Returns every unmatched settlement summed.
    #[must_use]
    pub const fn anomalies(&self) -> u64 {
        let mut total = 0;
        let mut index = 0;
        while index < self.anomalies.len() {
            total += self.anomalies[index];
            index += 1;
        }
        total
    }

    /// Returns whether every live count has settled to zero.
    ///
    /// Deliberately independent of the lifecycle state so a caller can
    /// distinguish "declared closed" from "actually holds nothing".
    #[must_use]
    pub const fn is_quiescent(&self) -> bool {
        self.resident_entries == 0
            && self.resident_bytes == 0
            && self.inflight_loads == 0
            && self.waiters == 0
            && self.anomalies() == 0
    }
}

/// The single production owner of Bifrost storage and cache lifecycle signals.
///
/// One instance belongs to one [`BifrostStorage`](super::BifrostStorage), so
/// two simulated nodes sharing an OS process still publish independent
/// reconcilable totals. Totals live under one mutex rather than as atomics
/// because every emission already happens at a state transition and one
/// consistent snapshot is worth more here than uncontended increments.
#[derive(Debug, Default)]
pub(crate) struct BifrostStorageTelemetry {
    /// Reconcilable totals and live state published since construction.
    totals: Mutex<MetadataCacheSnapshot>,
}

impl BifrostStorageTelemetry {
    /// Publishes one cache decision as a counter, totals, and a trace event.
    ///
    /// A poisoned totals lock never fails the decision being observed: the
    /// metric is still emitted and only the reconcilable totals stop advancing,
    /// because losing observation is strictly better than refusing admitted
    /// work.
    pub(crate) fn record_cache_effect(&self, effect: CacheEffect, reason: CacheEffectReason) {
        metrics::counter!(
            "bifrost_storage_metadata_cache_effects_total",
            "effect" => effect.as_str(),
            "reason" => reason.as_str(),
        )
        .increment(1);
        let snapshot = self.mutate(|totals| {
            totals.effects[effect.index()] = totals.effects[effect.index()].saturating_add(1);
            totals.reasons[reason.index()] = totals.reasons[reason.index()].saturating_add(1);
        });
        tracing::debug!(
            effect = effect.as_str(),
            reason = reason.as_str(),
            resident_entries = snapshot.resident_entries(),
            resident_bytes = snapshot.resident_bytes(),
            inflight_loads = snapshot.inflight_loads(),
            waiters = snapshot.waiters(),
            lifecycle = snapshot.lifecycle().as_str(),
            "Bifrost storage metadata lifecycle"
        );
    }

    /// Records one owning caller taking a metadata load.
    ///
    /// Raises the in-flight gauge until [`Self::record_load_terminal`] settles
    /// it, so starts and terminals reconcile exactly on a drained owner.
    pub(crate) fn record_load_start(&self) {
        let snapshot = self.mutate(|totals| {
            totals.load_starts = totals.load_starts.saturating_add(1);
            totals.inflight_loads = totals.inflight_loads.saturating_add(1);
        });
        Self::publish_gauges(&snapshot);
    }

    /// Records one terminal metadata load with its outcome and duration.
    pub(crate) fn record_load_terminal(&self, outcome: MetadataLoadOutcome, elapsed: Duration) {
        metrics::counter!(
            "bifrost_storage_metadata_cache_loads_total",
            "outcome" => outcome.as_str(),
        )
        .increment(1);
        metrics::histogram!(
            "bifrost_storage_metadata_load_seconds",
            "outcome" => outcome.as_str(),
        )
        .record(elapsed.as_secs_f64());
        let snapshot = self.mutate(|totals| {
            totals.load_terminals[outcome.index()] =
                totals.load_terminals[outcome.index()].saturating_add(1);
            settle(
                &mut totals.inflight_loads,
                TelemetryTransition::LoadTerminal,
                &mut totals.anomalies,
            );
        });
        Self::publish_gauges(&snapshot);
    }

    /// Records one joined waiter attaching to an in-flight load.
    pub(crate) fn record_waiter_joined(&self) {
        let snapshot = self.mutate(|totals| {
            totals.waiters = totals.waiters.saturating_add(1);
        });
        Self::publish_gauges(&snapshot);
    }

    /// Records one joined waiter leaving, whether settled or cancelled.
    pub(crate) fn record_waiter_settled(&self, outcome: MetadataLoadOutcome, elapsed: Duration) {
        metrics::histogram!(
            "bifrost_storage_metadata_wait_seconds",
            "outcome" => outcome.as_str(),
        )
        .record(elapsed.as_secs_f64());
        let snapshot = self.mutate(|totals| {
            settle(
                &mut totals.waiters,
                TelemetryTransition::WaiterSettled,
                &mut totals.anomalies,
            );
        });
        Self::publish_gauges(&snapshot);
    }

    /// Republishes the resident-entry and resident-byte gauges.
    ///
    /// Called by the cache after every retention change so the gauges describe
    /// the state the cache actually holds rather than a value derived by
    /// counting decisions.
    pub(crate) fn record_resident(&self, entries: u64, bytes: u64) {
        let snapshot = self.mutate(|totals| {
            totals.resident_entries = entries;
            totals.resident_bytes = bytes;
        });
        Self::publish_gauges(&snapshot);
    }

    /// Records one lifecycle transition of the storage owner.
    pub(crate) fn record_lifecycle(&self, lifecycle: StorageLifecycle) {
        let snapshot = self.mutate(|totals| {
            totals.lifecycle = lifecycle;
        });
        tracing::debug!(
            lifecycle = lifecycle.as_str(),
            resident_entries = snapshot.resident_entries(),
            inflight_loads = snapshot.inflight_loads(),
            waiters = snapshot.waiters(),
            "Bifrost storage metadata lifecycle"
        );
    }

    /// Returns the retained totals and live state.
    ///
    /// A poisoned totals lock yields the default view rather than unwinding: a
    /// lost observation must never fail the work being observed.
    pub(crate) fn snapshot(&self) -> MetadataCacheSnapshot {
        self.totals
            .lock()
            .map_or_else(|_| MetadataCacheSnapshot::default(), |totals| *totals)
    }

    /// Applies one mutation to the retained totals and returns the new view.
    ///
    /// The mutation and the read happen under one acquisition so a published
    /// gauge describes one consistent moment rather than two. A poisoned lock
    /// yields the default view, which stops the totals advancing without
    /// failing the transition being observed.
    fn mutate(&self, apply: impl FnOnce(&mut MetadataCacheSnapshot)) -> MetadataCacheSnapshot {
        let Ok(mut totals) = self.totals.lock() else {
            return MetadataCacheSnapshot::default();
        };
        apply(&mut totals);
        *totals
    }

    /// Publishes every live gauge from one consistent retained view.
    fn publish_gauges(snapshot: &MetadataCacheSnapshot) {
        metrics::gauge!("bifrost_storage_metadata_cache_resident_entries")
            .set(gauge_value(snapshot.resident_entries()));
        metrics::gauge!("bifrost_storage_metadata_cache_resident_bytes")
            .set(gauge_value(snapshot.resident_bytes()));
        metrics::gauge!("bifrost_storage_metadata_cache_inflight_loads")
            .set(gauge_value(snapshot.inflight_loads()));
        metrics::gauge!("bifrost_storage_metadata_cache_waiters")
            .set(gauge_value(snapshot.waiters()));
    }
}

/// Settles one live count by exactly one admission, checked rather than clamped.
///
/// A saturating decrement of a zero gauge is indistinguishable from a correct
/// one, which is how a broken owner reports a clean drain. This records the
/// mismatch as a bounded anomaly instead, so reconciliation failure is visible
/// in both the totals and an emitted counter, and leaves the count at zero
/// because that is still the only defensible value.
fn settle(
    count: &mut u64,
    transition: TelemetryTransition,
    anomalies: &mut [u64; TelemetryTransition::ALL.len()],
) {
    match count.checked_sub(1) {
        Some(settled) => *count = settled,
        None => {
            anomalies[transition.index()] = anomalies[transition.index()].saturating_add(1);
            metrics::counter!(
                "bifrost_storage_metadata_cache_transition_anomalies_total",
                "transition" => transition.as_str(),
            )
            .increment(1);
            tracing::warn!(
                transition = transition.as_str(),
                "Bifrost storage metadata telemetry settled an unmatched transition"
            );
        }
    }
}

/// Projects one retained count into a gauge value without silent truncation.
///
/// A count beyond `f64`'s exact integer range saturates to `f64::MAX` so an
/// impossible value is visibly wrong rather than quietly rounded.
fn gauge_value(value: u64) -> f64 {
    value.to_f64().unwrap_or(f64::MAX)
}
