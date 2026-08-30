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
    /// Returns the retained totals and live state.
    ///
    /// A poisoned totals lock yields the default view rather than unwinding: a
    /// lost observation must never fail the work being observed.
    pub(crate) fn snapshot(&self) -> MetadataCacheSnapshot {
        self.totals
            .lock()
            .map_or_else(|_| MetadataCacheSnapshot::default(), |totals| *totals)
    }
}
