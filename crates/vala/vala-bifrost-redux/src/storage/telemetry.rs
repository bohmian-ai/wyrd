//! Production metric emission for the node's one Bifrost storage owner.
//!
//! Every function here is called at the real transition it measures: a cache
//! decision, a metadata load terminal, a waiter leaving, a governed request
//! being admitted, retried, or ending, and a change to the state the cache or
//! request settlement actually holds. Nothing here keeps state of its own. The
//! live counts a drained node is checked against are read from the owners —
//! [`ParquetMetadataCache`](super::cache::ParquetMetadataCache) and the
//! storage owner's request settlement — never from a parallel ledger.
//!
//! Counters are process-lifetime totals: they reset when the process restarts
//! and are operational rates, not exact durable accounting.
//!
//! Tenant, table, path, checksum, snapshot, query, and request identities are
//! never metric labels. Scrubbed identity belongs on the caller's own span.

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
    /// The Oracle memory root would not fund retaining this metadata.
    ///
    /// The caller still receives the decode; the node simply declines to keep
    /// bytes it cannot account for, which is what keeps the cache inside the
    /// same managed-memory root as the queries it serves.
    Unfunded,
}

impl CacheEffectReason {
    /// Returns the emitted `reason` label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Disabled => "disabled",
            Self::Oversized => "oversized",
            Self::Closing => "closing",
            Self::Unfunded => "unfunded",
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

/// One governed object-store operation the storage owner admits.
///
/// The eleven members are the complete inventory of what any caller — the
/// Iceberg adapter included — may ask the owner to perform, split by whether
/// another attempt is allowed. The first five are idempotent reads of immutable
/// objects; the remaining six are one-attempt effects whose replay could
/// duplicate a durable write. The domain is closed and `'static` on purpose:
/// it is emitted as a metric label, so it must never widen with a path, a
/// tenant, or backend text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageOperation {
    /// Probe whether one object exists.
    Exists,
    /// Read one object's size and modification metadata.
    Stat,
    /// Read one object in full.
    Read,
    /// Read one byte range of an object.
    ReadRange,
    /// List the entries beneath one prefix.
    List,
    /// Write one object's complete bytes in a single effect.
    Write,
    /// Open a streaming writer over one object.
    OpenWriter,
    /// Append one buffer through an already-open writer.
    WriterWrite,
    /// Finalize an already-open writer, publishing the object.
    WriterClose,
    /// Delete one object.
    Delete,
    /// Delete every object beneath one prefix.
    DeletePrefix,
}

impl StorageOperation {
    /// Returns the emitted `operation` label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exists => "exists",
            Self::Stat => "stat",
            Self::Read => "read",
            Self::ReadRange => "read_range",
            Self::List => "list",
            Self::Write => "write",
            Self::OpenWriter => "open_writer",
            Self::WriterWrite => "writer_write",
            Self::WriterClose => "writer_close",
            Self::Delete => "delete",
            Self::DeletePrefix => "delete_prefix",
        }
    }

    /// Returns whether the owner may admit another attempt of this operation.
    ///
    /// Only the idempotent reads of immutable objects retry. Every effect is
    /// one-attempt because a transparently replayed write, writer append,
    /// close, or delete can publish or destroy an object twice, and the owner
    /// cannot tell a request that never landed from one whose acknowledgement
    /// was lost.
    #[must_use]
    pub const fn is_retryable(self) -> bool {
        matches!(
            self,
            Self::Exists | Self::Stat | Self::Read | Self::ReadRange | Self::List
        )
    }
}

/// How one logical governed storage request ended.
///
/// Mirrors the closed [`BifrostStorageError`](crate::storage::BifrostStorageError)
/// vocabulary plus success, so every admitted request publishes exactly one of
/// these and a settled owner's terminals reconcile against its starts by
/// equality rather than by judgement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageRequestOutcome {
    /// The operation returned its result.
    Success,
    /// The object does not exist at the validated location.
    NotFound,
    /// The backend refused the request for authorization reasons.
    PermissionDenied,
    /// One attempt exceeded the per-attempt request timeout.
    Timeout,
    /// Node-wide admission or the backend refused for rate reasons.
    RateLimited,
    /// Bytes were returned but did not decode as the expected format.
    InvalidData,
    /// Any other backend failure, including a panicked attempt.
    Backend,
    /// A caller token fired before a terminal result.
    Cancelled,
    /// The fixed absolute read bound elapsed before a terminal result.
    Deadline,
    /// The owner is closing or closed and admitted no work.
    Closed,
    /// The configured policy or locator is not usable.
    InvalidConfiguration,
}

impl StorageRequestOutcome {
    /// Returns the emitted `outcome` label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::NotFound => "not_found",
            Self::PermissionDenied => "permission_denied",
            Self::Timeout => "timeout",
            Self::RateLimited => "rate_limited",
            Self::InvalidData => "invalid_data",
            Self::Backend => "backend",
            Self::Cancelled => "cancelled",
            Self::Deadline => "deadline",
            Self::Closed => "closed",
            Self::InvalidConfiguration => "invalid_configuration",
        }
    }
}

/// The storage owner's lifecycle state, read from the owner that holds it.
///
/// The metadata cache keeps its own copy under its state lock, which is what
/// gates admission; [`BifrostStorage`](super::BifrostStorage) derives its
/// test-support view from its owner token and request settlement. It is never
/// a metric label and never a retained total.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageLifecycle {
    /// Accepting new loads.
    Open,
    /// Admitting no new loads while outstanding loaders settle.
    Closing,
    /// Fully settled; no entries, loads, waiters, or tasks remain.
    Closed,
}

/// Publishes one cache decision.
///
/// `bifrost_storage_metadata_cache_effects_total{effect,reason}` counts
/// lookups, not backend reads: a hit performs no object I/O and so never
/// appears in `bifrost_storage_requests_total`.
pub(crate) fn record_cache_effect(effect: CacheEffect, reason: CacheEffectReason) {
    metrics::counter!(
        "bifrost_storage_metadata_cache_effects_total",
        "effect" => effect.as_str(),
        "reason" => reason.as_str(),
    )
    .increment(1);
}

/// Publishes one terminal metadata load with its outcome and duration.
///
/// Called once by the loader task that owns the load, or once by a forced
/// close for a loader it had to abort before it could publish.
pub(crate) fn record_load_terminal(outcome: MetadataLoadOutcome, elapsed: Duration) {
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
}

/// Publishes how long one joined caller waited and how its wait ended.
pub(crate) fn record_waiter_settled(outcome: MetadataLoadOutcome, elapsed: Duration) {
    metrics::histogram!(
        "bifrost_storage_metadata_wait_seconds",
        "outcome" => outcome.as_str(),
    )
    .record(elapsed.as_secs_f64());
}

/// Sets the resident-entry and resident-byte gauges from the cache's state.
///
/// The cache calls this under its state lock after a retention change, so
/// the gauges describe what the cache holds rather than a value derived by
/// counting decisions.
pub(crate) fn record_resident(entries: u64, bytes: u64) {
    metrics::gauge!("bifrost_storage_metadata_cache_resident_entries").set(gauge_value(entries));
    metrics::gauge!("bifrost_storage_metadata_cache_resident_bytes").set(gauge_value(bytes));
}

/// Sets the in-flight load gauge from the cache's in-flight map size.
///
/// Called under the cache's state lock whenever the map gains or loses an
/// entry.
pub(crate) fn record_inflight_loads(loads: usize) {
    metrics::gauge!("bifrost_storage_metadata_cache_inflight_loads")
        .set(gauge_value(u64::try_from(loads).unwrap_or(u64::MAX)));
}

/// Publishes one logical governed storage request being admitted.
///
/// Called once per logical operation, never once per attempt, so a retried
/// read is one request. Raises `bifrost_storage_active_requests`, which the
/// matching [`record_request_terminal`] lowers; both are driven by the request
/// settlement that teardown waits on, so the gauge moves with that owner.
pub(crate) fn record_request_start(operation: StorageOperation) {
    metrics::counter!(
        "bifrost_storage_requests_total",
        "operation" => operation.as_str(),
    )
    .increment(1);
    metrics::gauge!("bifrost_storage_active_requests").increment(1.0);
}

/// Publishes the one terminal outcome of a logical governed request.
///
/// Both labels come from closed enums, so no path, tenant, table, checksum,
/// snapshot, query identifier, or backend error text can reach the emitted
/// cardinality through this boundary.
pub(crate) fn record_request_terminal(
    operation: StorageOperation,
    outcome: StorageRequestOutcome,
    elapsed: Duration,
) {
    metrics::counter!(
        "bifrost_storage_request_terminals_total",
        "operation" => operation.as_str(),
        "outcome" => outcome.as_str(),
    )
    .increment(1);
    metrics::histogram!(
        "bifrost_storage_request_seconds",
        "operation" => operation.as_str(),
        "outcome" => outcome.as_str(),
    )
    .record(elapsed.as_secs_f64());
    metrics::gauge!("bifrost_storage_active_requests").decrement(1.0);
}

/// Publishes one retried attempt of an idempotent governed read.
///
/// Published at the moment the owner admits the attempt, so the total is the
/// count of attempts beyond the first rather than of retry decisions that a
/// deadline or cancellation then refused.
pub(crate) fn record_request_retry(operation: StorageOperation) {
    metrics::counter!(
        "bifrost_storage_request_retries_total",
        "operation" => operation.as_str(),
    )
    .increment(1);
}

/// Projects one owned count into a gauge value without silent truncation.
///
/// A count beyond `f64`'s exact integer range saturates to `f64::MAX` so an
/// impossible value is visibly wrong rather than quietly rounded.
fn gauge_value(value: u64) -> f64 {
    value.to_f64().unwrap_or(f64::MAX)
}

/// Reads the storage families back out of a scoped test recorder.
///
/// Storage unit tests install a [`wyrd_bench::BenchmarkRecorder`] as the
/// thread's local recorder on a current-thread runtime, so every emission the
/// production paths above make — including from spawned loader tasks — lands
/// in it. These helpers only read; they never emit.
#[cfg(test)]
pub(crate) mod recorded {
    use wyrd_bench::BenchmarkRecorder;

    /// Logical governed requests admitted.
    pub(crate) const REQUESTS: &str = "bifrost_storage_requests_total";
    /// Logical governed request terminals.
    pub(crate) const REQUEST_TERMINALS: &str = "bifrost_storage_request_terminals_total";
    /// Attempts admitted after a read's first.
    pub(crate) const REQUEST_RETRIES: &str = "bifrost_storage_request_retries_total";
    /// Logical governed requests admitted and not yet settled.
    pub(crate) const ACTIVE_REQUESTS: &str = "bifrost_storage_active_requests";
    /// Cache decisions.
    pub(crate) const CACHE_EFFECTS: &str = "bifrost_storage_metadata_cache_effects_total";
    /// Metadata load terminals.
    pub(crate) const CACHE_LOADS: &str = "bifrost_storage_metadata_cache_loads_total";
    /// Joined-caller wait latency.
    pub(crate) const WAIT_SECONDS: &str = "bifrost_storage_metadata_wait_seconds";
    /// Retired: the unmatched-settlement counter the shadow ledger published.
    pub(crate) const RETIRED_ANOMALIES: &str =
        "bifrost_storage_metadata_cache_transition_anomalies_total";
    /// Retired: the waiter gauge republished on every transition.
    pub(crate) const RETIRED_WAITERS: &str = "bifrost_storage_metadata_cache_waiters";

    /// Returns whether one rendered series belongs to `family` and carries
    /// every `labels` pair.
    fn matches(series: &str, family: &str, labels: &[(&str, &str)]) -> bool {
        let Some(rest) = series.strip_prefix(family) else {
            return false;
        };
        if !(rest.is_empty() || rest.starts_with('{')) {
            return false;
        }
        labels.iter().all(|(key, value)| {
            let pair = format!("{key}=\"{value}\"");
            rest.contains(&format!("{{{pair}")) || rest.contains(&format!(",{pair}"))
        })
    }

    /// Sums every counter series of `family` carrying all `labels`.
    pub(crate) fn counter(
        recorder: &BenchmarkRecorder,
        family: &str,
        labels: &[(&str, &str)],
    ) -> u64 {
        recorder
            .snapshot()
            .counters
            .iter()
            .filter(|(series, _)| matches(series, family, labels))
            .map(|(_, value)| value)
            .sum()
    }

    /// Sums the observation counts of every histogram series of `family`
    /// carrying all `labels`.
    pub(crate) fn observations(
        recorder: &BenchmarkRecorder,
        family: &str,
        labels: &[(&str, &str)],
    ) -> u64 {
        recorder
            .snapshot()
            .histograms
            .iter()
            .filter(|(series, _)| matches(series, family, labels))
            .map(|(_, histogram)| histogram.count)
            .sum()
    }

    /// Returns the current value of one unlabelled gauge, or zero when unset.
    pub(crate) fn gauge(recorder: &BenchmarkRecorder, family: &str) -> f64 {
        recorder
            .snapshot()
            .gauges
            .get(family)
            .copied()
            .unwrap_or_default()
    }

    /// Returns whether any series of `family` was ever registered.
    pub(crate) fn emitted(recorder: &BenchmarkRecorder, family: &str) -> bool {
        recorder.snapshot().contains_family(family)
    }
}
