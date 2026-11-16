//! Per-server Gateway quota usage held in memory.
//!
//! [`GatewayUsage`] keeps, per tenant, the usage this process charged and the
//! last shared view of every other server's usage, and admits quota-covered
//! calls against their sum under one short lock, with no IO. Background work
//! ([`GatewayUsage::run`]) persists this process's contribution and reads the
//! shared view back on one tenant transaction
//! ([`wyrd_sql::queries::gateway_admission::sync_gateway_usage`]).
//!
//! Quotas are approximate across servers: servers may together exceed a
//! configured total before their usage becomes mutually visible. Locally an
//! admission can never spend headroom another local admission already took.
//!
//! Window and concurrency usage is persisted as this process's absolute
//! totals under its random `server` identity, and the shared view excludes
//! that identity, so neither a retried write nor a refresh counts a local
//! charge twice. Budget spend is read from the reservation ledger, which
//! includes this process's committed entries; each local reservation counts
//! only the part of its cost the last committed view does not include.
//!
//! A tenant with no shared view, a view older than [`STALE_AFTER`], or
//! [`PENDING_ENTRIES`] unpersisted ledger entries refuses covered calls with
//! the retryable 503 and wakes background recovery. Usage not yet persisted
//! is lost if the process dies; it is never written to local storage.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use chrono::{DateTime, TimeDelta, Utc};
use futures_util::StreamExt as _;
use lru::LruCache;
use serde_json::json;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use wyrd_gateway::GatewayCost;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::GatewayPolicySubject;
use wyrd_sql::WyrdPostgres;
use wyrd_sql::queries::gateway_admission::{
    GatewayBudgetSpend, GatewayUsageConcurrency, GatewayUsageEntry, GatewayUsageView,
    GatewayUsageWindow, GatewayUsageWrite, sync_gateway_usage,
};

/// A shared view older than this refuses covered calls.
pub(crate) const STALE_AFTER: Duration = Duration::from_secs(30);

/// Unpersisted ledger entries one tenant may hold before refusing calls.
pub(crate) const PENDING_ENTRIES: usize = 4_096;

/// Refresh interval of a tenant used within [`RECENT`] or holding unpersisted
/// usage or running calls.
const ACTIVE_REFRESH: Duration = Duration::from_secs(1);

/// Refresh interval of any other retained tenant, well inside
/// [`STALE_AFTER`].
const IDLE_REFRESH: Duration = Duration::from_secs(10);

/// How recently a tenant must have admitted a call to refresh actively.
const RECENT: Duration = Duration::from_secs(60);

/// Wait between [`GatewayUsage::drain`] attempts to persist usage a failed
/// synchronization left behind.
const DRAIN_RETRY: Duration = Duration::from_millis(250);

/// How long a persisted concurrency count lasts unless renewed.
const CONCURRENCY_LEASE: Duration = Duration::from_secs(30);

/// Most tenants whose usage one process retains.
const TENANTS: NonZeroUsize = NonZeroUsize::new(4_096).expect("4096 is non-zero");

/// Most tenants synchronized at once.
const SYNC_CONCURRENCY: usize = 8;

/// One limit an admission checks and charges.
#[derive(Debug)]
pub(crate) struct LimitCharge {
    /// Canonical JSON of the limit's `(subject, target)` key.
    pub key: String,
    /// Requests per window, or `None` when unset.
    pub requests: Option<u64>,
    /// Tokens per window, or `None` when unset.
    pub tokens: Option<u64>,
    /// Concurrent calls, or `None` when unset.
    pub concurrent: Option<u64>,
    /// Token exposure the call holds in the window.
    pub hold: i64,
    /// Whether the call counts against the limit's concurrency.
    pub lease: bool,
}

/// One budget an admission checks and reserves against.
#[derive(Debug)]
pub(crate) struct BudgetCharge {
    /// Budget subject and period key; see [`budget_key`].
    pub key: String,
    /// Budget amount.
    pub amount: GatewayCost,
    /// Cost the call reserves.
    pub reserve: GatewayCost,
    /// The reservation's identity and creation entry, or `None` for a zero
    /// reserve.
    pub reservation: Option<(Uuid, GatewayUsageEntry)>,
}

/// One reservation's settlement.
#[derive(Debug)]
pub(crate) struct Settled {
    /// Reservation identity.
    pub reservation: Uuid,
    /// Actual cost the reservation now counts.
    pub actual: GatewayCost,
    /// The settlement entry.
    pub entry: GatewayUsageEntry,
}

/// The decision of one local admission.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Every limit and budget had room; all were charged.
    Admitted,
    /// These limit keys are exhausted; nothing was charged.
    Exhausted(Vec<String>),
    /// No limit is exhausted but a budget cannot cover its reserve; nothing
    /// was charged.
    BudgetExceeded,
}

/// The key matching a budget's local reservations to its committed spend.
pub(crate) fn budget_key(
    subject: &GatewayPolicySubject,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
) -> String {
    json!([subject, period_start, period_end]).to_string()
}

/// The [`budget_key`] of one committed spend row, or `None` when its stored
/// subject or period no longer decodes.
fn spend_key(row: &GatewayBudgetSpend) -> Option<String> {
    let subject = serde_json::from_value(row.subject.clone()).ok()?;
    let at = |text: &str| {
        DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|at| at.to_utc())
    };
    Some(budget_key(
        &subject,
        at(&row.period_start)?,
        at(&row.period_end)?,
    ))
}

/// Requests and tokens in one window.
#[derive(Debug, Clone, Copy, Default)]
struct Counts {
    /// Requests charged.
    requests: i64,
    /// Tokens charged or held.
    tokens: i64,
}

/// Every other server's usage as of one committed refresh.
struct Shared {
    /// Window totals by `(limit key, window start)`.
    windows: HashMap<(String, DateTime<Utc>), Counts>,
    /// Running calls by limit key.
    active: HashMap<String, i64>,
    /// Committed spend by [`budget_key`], this process's committed entries
    /// included.
    spend: HashMap<String, GatewayCost>,
    /// When the refresh committed.
    at: Instant,
}

/// One budget reservation this process made.
struct Local {
    /// The budget's [`budget_key`].
    budget: String,
    /// Cost it counts: reserved, then actual once settled.
    cost: GatewayCost,
    /// Cost the shared view already includes.
    acked: GatewayCost,
    /// Creation entry not yet persisted.
    created: Option<GatewayUsageEntry>,
    /// Settlement entry not yet persisted.
    settled: Option<GatewayUsageEntry>,
}

/// One tenant's usage.
#[derive(Default)]
struct TenantUsage {
    /// Last committed shared view; `None` until the first refresh.
    shared: Option<Shared>,
    /// This process's absolute window totals.
    windows: HashMap<(String, DateTime<Utc>), Counts>,
    /// This process's running calls by limit key.
    active: HashMap<String, i64>,
    /// This process's reservations the shared view does not fully include.
    reservations: HashMap<Uuid, Local>,
    /// Unpersisted ledger entries.
    pending: usize,
    /// Whether windows or concurrency changed since the last persisted
    /// batch was taken.
    dirty: bool,
    /// Serializes the tenant's synchronizations, so a later one never
    /// commits or installs before an earlier one.
    ///
    /// Each [`GatewayUsage::sync`] holds a clone from before it waits until
    /// it returns or is dropped, and a tenant with an outstanding clone is
    /// never [`clean`](Self::clean), so eviction cannot replace this
    /// serialization while a synchronization may still install.
    sync: Arc<tokio::sync::Mutex<()>>,
    /// When the tenant last admitted a covered call.
    used: Option<Instant>,
}

impl TenantUsage {
    /// Whether nothing would be lost by forgetting the tenant: no usage is
    /// held and no synchronization is in flight.
    fn clean(&self) -> bool {
        Arc::strong_count(&self.sync) == 1
            && !self.dirty
            && self.pending == 0
            && self.windows.is_empty()
            && self.active.is_empty()
            && self.reservations.is_empty()
    }

    /// Whether the tenant holds usage no committed synchronization has
    /// persisted yet; committed window and concurrency totals are not.
    fn unpersisted(&self) -> bool {
        self.dirty || self.pending > 0
    }

    /// Whether the tenant is due a synchronization at `now`.
    fn due(&self, now: Instant) -> bool {
        let Some(shared) = &self.shared else {
            return true;
        };
        let busy = self.dirty
            || self.pending > 0
            || !self.active.is_empty()
            || self
                .used
                .is_some_and(|used| now.duration_since(used) < RECENT);
        let interval = if busy { ACTIVE_REFRESH } else { IDLE_REFRESH };
        now.duration_since(shared.at) >= interval
    }
}

/// What one synchronization persists, taken under the lock.
struct Batch {
    /// Absolute window totals.
    windows: Vec<GatewayUsageWindow>,
    /// Running calls per limit.
    concurrency: Vec<GatewayUsageConcurrency>,
    /// Ledger entries.
    entries: Vec<GatewayUsageEntry>,
    /// Each persisted reservation's identity, whether its entry settles it,
    /// and the cost the entry commits.
    acked: Vec<(Uuid, bool, GatewayCost)>,
    /// Windows starting before this are pruned.
    floor: DateTime<Utc>,
}

/// Process-wide Gateway quota usage; see the module docs.
// ponytail: one process-wide lock held for a few map operations per call;
// shard by tenant if admission contention shows up in profiles.
pub struct GatewayUsage {
    /// This process's contribution identity.
    server: Uuid,
    /// Tenants' usage, least recently used evicted first while clean.
    tenants: Mutex<LruCache<DataTenantId, TenantUsage>>,
    /// Wakes [`Self::run`] for a tenant that needs its first view.
    wake: Notify,
}

impl Default for GatewayUsage {
    /// No tenants under a fresh random server identity.
    fn default() -> Self {
        Self {
            server: Uuid::new_v4(),
            tenants: Mutex::new(LruCache::new(TENANTS)),
            wake: Notify::new(),
        }
    }
}

impl GatewayUsage {
    /// Locks the tenants; a poisoned lock is recovered, because every holder
    /// leaves them consistent before anything under it could panic.
    fn tenants(&self) -> MutexGuard<'_, LruCache<DataTenantId, TenantUsage>> {
        self.tenants.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Retains `tenant`, evicting the least recently used clean tenant when
    /// full, and returns whether it is retained.
    fn retain(tenants: &mut LruCache<DataTenantId, TenantUsage>, tenant: DataTenantId) -> bool {
        if tenants.contains(&tenant) {
            return true;
        }
        if tenants.len() >= tenants.cap().get() {
            match tenants.peek_lru() {
                Some((_, lru)) if lru.clean() => {
                    tenants.pop_lru();
                }
                _ => return false,
            }
        }
        tenants.put(tenant, TenantUsage::default());
        true
    }

    /// Checks `limits` and `budgets` against local plus shared usage in
    /// `window_start` and charges all of them only when all have room.
    ///
    /// Charges stay in memory until [`Self::run`] persists them.
    ///
    /// # Errors
    /// Returns `ServiceUnavailable`, and wakes background recovery, when the
    /// tenant has no usable shared view or its pending entries are full.
    pub(crate) fn admit(
        &self,
        tenant: DataTenantId,
        window_start: DateTime<Utc>,
        limits: &[LimitCharge],
        budgets: Vec<BudgetCharge>,
    ) -> Result<Outcome, WyrdError> {
        let now = Instant::now();
        let mut tenants = self.tenants();
        if !Self::retain(&mut tenants, tenant) {
            return Err(refused("gateway usage capacity is full"));
        }
        let usage = tenants
            .get_mut(&tenant)
            .expect("a retained tenant is present");
        let Some(shared) = usage
            .shared
            .as_ref()
            .filter(|shared| now.duration_since(shared.at) < STALE_AFTER)
        else {
            drop(tenants);
            self.wake.notify_one();
            return Err(refused("gateway usage is not yet loaded"));
        };
        let entries = budgets.iter().filter(|b| b.reservation.is_some()).count();
        if usage.pending + entries > PENDING_ENTRIES {
            drop(tenants);
            self.wake.notify_one();
            return Err(refused("gateway usage awaiting persistence is full"));
        }

        let cap = |value: Option<u64>| value.map(|cap| i64::try_from(cap).unwrap_or(i64::MAX));
        let mut exhausted = Vec::new();
        for limit in limits {
            let window = (limit.key.clone(), window_start);
            let local = usage.windows.get(&window).copied().unwrap_or_default();
            let other = shared.windows.get(&window).copied().unwrap_or_default();
            let requests = local.requests.saturating_add(other.requests);
            let tokens = local.tokens.saturating_add(other.tokens);
            let running = usage.active.get(&limit.key).copied().unwrap_or(0)
                + shared.active.get(&limit.key).copied().unwrap_or(0);
            if cap(limit.requests).is_some_and(|cap| cap <= requests)
                || cap(limit.tokens).is_some_and(|cap| cap < tokens.saturating_add(limit.hold))
                || cap(limit.concurrent).is_some_and(|cap| cap <= running)
            {
                exhausted.push(limit.key.clone());
            }
        }
        if !exhausted.is_empty() {
            return Ok(Outcome::Exhausted(exhausted));
        }
        for budget in &budgets {
            let committed = shared
                .spend
                .get(&budget.key)
                .cloned()
                .unwrap_or_else(GatewayCost::zero);
            let local: GatewayCost = usage
                .reservations
                .values()
                .filter(|local| local.budget == budget.key)
                .map(|local| local.cost.saturating_sub(&local.acked))
                .sum();
            if committed + local + budget.reserve.clone() > budget.amount {
                return Ok(Outcome::BudgetExceeded);
            }
        }

        for limit in limits {
            let counts = usage
                .windows
                .entry((limit.key.clone(), window_start))
                .or_default();
            counts.requests = counts.requests.saturating_add(1);
            counts.tokens = counts.tokens.saturating_add(limit.hold);
            if limit.lease {
                *usage.active.entry(limit.key.clone()).or_default() += 1;
            }
        }
        for budget in budgets {
            if let Some((id, entry)) = budget.reservation {
                usage.reservations.insert(
                    id,
                    Local {
                        budget: budget.key,
                        cost: budget.reserve,
                        acked: GatewayCost::zero(),
                        created: Some(entry),
                        settled: None,
                    },
                );
            }
        }
        usage.pending += entries;
        usage.dirty = true;
        usage.used = Some(now);
        Ok(Outcome::Admitted)
    }

    /// Applies one call's settlement in memory: corrects each token hold in
    /// `window_start` by its delta (never below zero), ends the call's
    /// concurrency under `leases`, and settles its reservations.
    ///
    /// A settlement whose tenant or window is no longer retained changes
    /// nothing there; such a window has already closed.
    pub(crate) fn settle(
        &self,
        tenant: DataTenantId,
        window_start: DateTime<Utc>,
        holds: &[(String, i64)],
        leases: &[String],
        settled: Vec<Settled>,
    ) {
        let mut tenants = self.tenants();
        let Some(usage) = tenants.get_mut(&tenant) else {
            return;
        };
        for (key, delta) in holds {
            if let Some(counts) = usage.windows.get_mut(&(key.clone(), window_start)) {
                counts.tokens = counts.tokens.saturating_add(*delta).max(0);
            }
        }
        for key in leases {
            if let Some(running) = usage.active.get_mut(key) {
                *running -= 1;
                if *running <= 0 {
                    usage.active.remove(key);
                }
            }
        }
        for settlement in settled {
            if let Some(local) = usage.reservations.get_mut(&settlement.reservation)
                && local.settled.is_none()
            {
                local.cost = settlement.actual;
                local.settled = Some(settlement.entry);
                usage.pending += 1;
            }
        }
        usage.dirty = true;
    }

    /// Takes `tenant`'s next batch, or `None` when it is not retained.
    fn batch(&self, tenant: DataTenantId) -> Option<Batch> {
        let mut tenants = self.tenants();
        let usage = tenants.peek_mut(&tenant)?;
        usage.dirty = false;
        let mut entries = Vec::new();
        let mut acked = Vec::new();
        for (id, local) in &usage.reservations {
            if let Some(entry) = &local.settled {
                entries.extend(local.created.clone());
                entries.push(entry.clone());
                acked.push((*id, true, local.cost.clone()));
            } else if let Some(entry) = &local.created {
                entries.push(entry.clone());
                acked.push((*id, false, local.cost.clone()));
            }
        }
        Some(Batch {
            windows: usage
                .windows
                .iter()
                .map(|((key, start), counts)| GatewayUsageWindow {
                    limit_key: key.clone(),
                    window_start: *start,
                    requests: counts.requests,
                    tokens: counts.tokens,
                })
                .collect(),
            concurrency: usage
                .active
                .iter()
                .map(|(key, active)| GatewayUsageConcurrency {
                    limit_key: key.clone(),
                    active: *active,
                })
                .collect(),
            entries,
            acked,
            floor: window_floor(Utc::now()),
        })
    }

    /// Installs a committed `view` of `batch`: replaces the shared view,
    /// marks the batch's reservation entries persisted, forgets
    /// reservations the view now fully includes, and prunes closed windows.
    fn apply(&self, tenant: DataTenantId, batch: &Batch, view: GatewayUsageView) {
        let shared = Shared {
            windows: view
                .windows
                .into_iter()
                .map(|w| {
                    let counts = Counts {
                        requests: w.requests,
                        tokens: w.tokens,
                    };
                    ((w.limit_key, w.window_start), counts)
                })
                .collect(),
            active: view
                .concurrency
                .into_iter()
                .map(|c| (c.limit_key, c.active))
                .collect(),
            spend: view
                .spend
                .into_iter()
                .filter_map(|row| Some((spend_key(&row)?, GatewayCost::parse(&row.spend).ok()?)))
                .collect(),
            at: Instant::now(),
        };
        let mut tenants = self.tenants();
        let Some(usage) = tenants.peek_mut(&tenant) else {
            return;
        };
        usage.shared = Some(shared);
        for (id, settles, cost) in &batch.acked {
            let Some(local) = usage.reservations.get_mut(id) else {
                continue;
            };
            if local.created.take().is_some() {
                usage.pending -= 1;
            }
            local.acked = cost.clone();
            if *settles {
                usage.pending -= 1;
                usage.reservations.remove(id);
            }
        }
        usage.windows.retain(|(_, start), _| *start >= batch.floor);
    }

    /// Persists `tenant`'s usage and refreshes its shared view on one tenant
    /// transaction; a tenant not yet retained is retained first, so this
    /// also establishes a first view.
    ///
    /// Waits for an in-flight synchronization of the tenant first. A failed
    /// or uncertain commit installs nothing and keeps every pending charge
    /// for the next attempt, which replaces rows and fences entries rather
    /// than adding to them.
    ///
    /// # Errors
    /// Returns `ServiceUnavailable` when storage fails.
    pub(crate) async fn sync(
        &self,
        postgres: &WyrdPostgres,
        tenant: DataTenantId,
    ) -> Result<(), WyrdError> {
        let sync = {
            let mut tenants = self.tenants();
            if !Self::retain(&mut tenants, tenant) {
                return Err(refused("gateway usage capacity is full"));
            }
            tenants
                .peek(&tenant)
                .map(|usage| Arc::clone(&usage.sync))
                .expect("a retained tenant is present")
        };
        let _serial = sync.lock().await;
        let Some(batch) = self.batch(tenant) else {
            return Ok(());
        };
        let result = async {
            let mut conn = postgres.tenant_conn(tenant).await.map_err(unavailable)?;
            let view = sync_gateway_usage(
                &mut conn,
                &GatewayUsageWrite {
                    server_id: self.server,
                    windows: &batch.windows,
                    window_floor: batch.floor,
                    concurrency: &batch.concurrency,
                    concurrency_lease_seconds: CONCURRENCY_LEASE.as_secs_f64(),
                    entries: &batch.entries,
                },
            )
            .await
            .map_err(unavailable)?;
            conn.commit().await.map_err(unavailable)?;
            Ok(view)
        }
        .await;
        match result {
            Ok(view) => {
                self.apply(tenant, &batch, view);
                Ok(())
            }
            Err(error) => {
                if let Some(usage) = self.tenants().peek_mut(&tenant) {
                    usage.dirty = true;
                }
                Err(error)
            }
        }
    }

    /// Synchronizes every tenant `due` selects, at most
    /// [`SYNC_CONCURRENCY`] at once, logging failures.
    async fn sync_where(&self, postgres: &WyrdPostgres, due: impl Fn(&TenantUsage) -> bool) {
        let tenants: Vec<DataTenantId> = self
            .tenants()
            .iter()
            .filter(|(_, usage)| due(usage))
            .map(|(tenant, _)| *tenant)
            .collect();
        futures_util::stream::iter(tenants)
            .for_each_concurrent(SYNC_CONCURRENCY, |tenant| async move {
                if let Err(error) = self.sync(postgres, tenant).await {
                    tracing::warn!(tenant = %tenant.as_uuid(), error = %error, "gateway usage synchronization failed");
                }
            })
            .await;
    }

    /// Persists and refreshes due tenants every [`ACTIVE_REFRESH`], and at
    /// once for a tenant that needs its first view, until `shutdown`.
    pub(crate) async fn run(&self, postgres: &WyrdPostgres, shutdown: CancellationToken) {
        loop {
            tokio::select! {
                () = shutdown.cancelled() => return,
                () = self.wake.notified() => {}
                () = tokio::time::sleep(ACTIVE_REFRESH) => {}
            }
            let now = Instant::now();
            self.sync_where(postgres, |usage| usage.due(now)).await;
        }
    }

    /// Persists every tenant holding usage or running calls once and
    /// refreshes its shared view, logging failures.
    pub(crate) async fn flush(&self, postgres: &WyrdPostgres) {
        self.sync_where(postgres, |usage| !usage.clean()).await;
    }

    /// Flushes, then retries every [`DRAIN_RETRY`] each tenant whose usage
    /// is still unpersisted until all of it commits, as the final step of
    /// shutdown drain.
    ///
    /// Each retry replaces rows and fences entries, so a write that failed
    /// or committed uncertainly is counted once. Never returns while storage
    /// keeps failing; shutdown drain bounds it by its deadline.
    pub(crate) async fn drain(&self, postgres: &WyrdPostgres) {
        self.flush(postgres).await;
        while self.tenants().iter().any(|(_, usage)| usage.unpersisted()) {
            tokio::time::sleep(DRAIN_RETRY).await;
            self.sync_where(postgres, TenantUsage::unpersisted).await;
        }
    }
}

/// Start of the earliest window admission or settlement still reads: the
/// minute before the one containing `now`.
fn window_floor(now: DateTime<Utc>) -> DateTime<Utc> {
    let seconds = now.timestamp();
    DateTime::from_timestamp(seconds - seconds.rem_euclid(60), 0).unwrap_or(now)
        - TimeDelta::minutes(1)
}

/// The retryable refusal of a covered call without usable usage state.
fn refused(reason: &str) -> WyrdError {
    tracing::warn!(reason, "gateway call refused before dispatch");
    WyrdError::ServiceUnavailable {
        message: format!("{reason}; retry shortly"),
        details: json!({}),
    }
}

/// Maps a storage failure of background synchronization.
fn unavailable(error: impl std::fmt::Display) -> WyrdError {
    WyrdError::ServiceUnavailable {
        message: format!("gateway usage storage is unavailable: {error}"),
        details: json!({}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shared view with no other usage, committed now.
    ///
    /// # Panics
    /// Panics when `usage` is at capacity with no clean tenant to evict, so
    /// `tenant` is not retained.
    fn fresh(usage: &GatewayUsage, tenant: DataTenantId) {
        GatewayUsage::retain(&mut usage.tenants(), tenant);
        let batch = usage.batch(tenant).expect("batch");
        usage.apply(tenant, &batch, GatewayUsageView::default());
    }

    /// A one-request limit.
    fn one_request(key: &str) -> LimitCharge {
        LimitCharge {
            key: key.to_owned(),
            requests: Some(1),
            tokens: None,
            concurrent: None,
            hold: 0,
            lease: false,
        }
    }

    /// One-call concurrency.
    fn one_call(key: &str) -> LimitCharge {
        LimitCharge {
            key: key.to_owned(),
            requests: None,
            tokens: None,
            concurrent: Some(1),
            hold: 0,
            lease: true,
        }
    }

    /// A reservation creation entry.
    fn created() -> GatewayUsageEntry {
        GatewayUsageEntry {
            entry_id: Uuid::new_v4(),
            call_id: Uuid::new_v4(),
            kind: "budget_reservation_created",
            entry: json!({}),
        }
    }

    /// A tenant without a view refuses and is woken; admission charges
    /// local headroom once, keeps tenants apart, and releases concurrency on
    /// settlement.
    ///
    /// # Panics
    /// Panics when a decision differs from the one described.
    #[test]
    fn local_admission_spends_headroom_once_per_tenant() {
        let usage = GatewayUsage::default();
        let (tenant, other) = (DataTenantId::new_v7(), DataTenantId::new_v7());
        let window = window_floor(Utc::now());
        let refusal = usage
            .admit(tenant, window, &[one_request("k")], Vec::new())
            .expect_err("no view");
        assert_eq!(refusal.code(), "WYRD_SERVER_503_SERVICE_UNAVAILABLE");

        fresh(&usage, tenant);
        fresh(&usage, other);
        let admit = |tenant, limits: &[LimitCharge]| {
            usage
                .admit(tenant, window, limits, Vec::new())
                .expect("admits")
        };
        assert_eq!(admit(tenant, &[one_request("k")]), Outcome::Admitted);
        assert_eq!(
            admit(tenant, &[one_request("k")]),
            Outcome::Exhausted(vec!["k".to_owned()])
        );
        assert_eq!(admit(other, &[one_request("k")]), Outcome::Admitted);

        assert_eq!(admit(tenant, &[one_call("c")]), Outcome::Admitted);
        assert_eq!(
            admit(tenant, &[one_call("c")]),
            Outcome::Exhausted(vec!["c".to_owned()])
        );
        usage.settle(tenant, window, &[], &["c".to_owned()], Vec::new());
        assert_eq!(admit(tenant, &[one_call("c")]), Outcome::Admitted);
    }

    /// A reservation counts in full until committed, then only the part the
    /// committed spend lacks; settlement lowers it to its actual cost.
    ///
    /// # Panics
    /// Panics when a decision, acknowledgement, or pending count differs from
    /// the one described.
    #[test]
    fn budget_reservations_count_once_across_refreshes() {
        let usage = GatewayUsage::default();
        let tenant = DataTenantId::new_v7();
        let window = window_floor(Utc::now());
        fresh(&usage, tenant);
        let cost = |text: &str| GatewayCost::parse(text).expect("cost");
        let (start, end) = (window, window + TimeDelta::days(1));
        let key = budget_key(&GatewayPolicySubject::Tenant, start, end);
        let charge = |reserve: &str, reservation: Option<Uuid>| BudgetCharge {
            key: key.clone(),
            amount: cost("10"),
            reserve: cost(reserve),
            reservation: reservation.map(|id| (id, created())),
        };
        let first = Uuid::new_v4();
        assert_eq!(
            usage
                .admit(tenant, window, &[], vec![charge("6", Some(first))])
                .expect("admits"),
            Outcome::Admitted
        );
        assert_eq!(
            usage
                .admit(tenant, window, &[], vec![charge("5", None)])
                .expect("decides"),
            Outcome::BudgetExceeded
        );

        let batch = usage.batch(tenant).expect("batch");
        assert_eq!(batch.entries.len(), 1);
        let view = GatewayUsageView {
            spend: vec![GatewayBudgetSpend {
                subject: json!("tenant"),
                period_start: start.to_rfc3339(),
                period_end: end.to_rfc3339(),
                spend: "6".to_owned(),
            }],
            ..GatewayUsageView::default()
        };
        usage.apply(tenant, &batch, view);
        let local = usage
            .tenants()
            .peek(&tenant)
            .map(|usage| (usage.pending, usage.reservations[&first].acked.clone()))
            .expect("tenant");
        assert_eq!(local, (0, cost("6")), "committed creation is acknowledged");

        usage.settle(
            tenant,
            window,
            &[],
            &[],
            vec![Settled {
                reservation: first,
                actual: cost("2"),
                entry: created(),
            }],
        );
        let pending = usage.tenants().peek(&tenant).map(|usage| usage.pending);
        assert_eq!(pending, Some(1), "the settlement awaits persistence");
    }

    /// A view older than [`STALE_AFTER`] and a full pending buffer each
    /// refuse with the retryable 503, charge nothing, and wake background
    /// recovery; a committed refresh or acknowledgement restores admission.
    ///
    /// # Panics
    /// Panics when a refusal differs, charges, or does not wake recovery, or
    /// when recovery does not restore admission.
    #[test]
    fn stale_and_full_usage_refuse_until_background_recovery() {
        use futures_util::FutureExt as _;

        let usage = GatewayUsage::default();
        let tenant = DataTenantId::new_v7();
        let window = window_floor(Utc::now());
        let refused = |limits: &[LimitCharge], budgets: Vec<BudgetCharge>| {
            let error = usage
                .admit(tenant, window, limits, budgets)
                .expect_err("refused before charging");
            assert_eq!(error.code(), "WYRD_SERVER_503_SERVICE_UNAVAILABLE");
            assert!(
                usage.wake.notified().now_or_never().is_some(),
                "refusal wakes background recovery"
            );
        };

        fresh(&usage, tenant);
        if let Some(shared) = usage
            .tenants()
            .peek_mut(&tenant)
            .and_then(|usage| usage.shared.as_mut())
        {
            shared.at = Instant::now()
                .checked_sub(STALE_AFTER)
                .expect("the clock has run longer than the staleness bound");
        }
        refused(&[one_request("k")], Vec::new());
        fresh(&usage, tenant);

        let cost = |text: &str| GatewayCost::parse(text).expect("cost");
        let key = budget_key(
            &GatewayPolicySubject::Tenant,
            window,
            window + TimeDelta::days(1),
        );
        let reservation = || BudgetCharge {
            key: key.clone(),
            amount: cost("10"),
            reserve: cost("0"),
            reservation: Some((Uuid::new_v4(), created())),
        };
        let fill = (0..PENDING_ENTRIES).map(|_| reservation()).collect();
        assert_eq!(
            usage.admit(tenant, window, &[], fill).expect("fills"),
            Outcome::Admitted
        );
        refused(&[one_request("k")], vec![reservation()]);
        let batch = usage.batch(tenant).expect("batch");
        usage.apply(tenant, &batch, GatewayUsageView::default());

        assert_eq!(
            usage
                .admit(tenant, window, &[one_request("k")], vec![reservation()])
                .expect("admits"),
            Outcome::Admitted,
            "neither refusal charged the one-request limit"
        );
    }
}

#[cfg(test)]
mod pg_tests {
    use std::sync::Arc;
    use std::time::Duration;

    use wyrd_dev_fixtures::pg::PgFixture;

    use super::*;

    /// Usage retaining at most one tenant.
    fn one_tenant() -> Arc<GatewayUsage> {
        Arc::new(GatewayUsage {
            tenants: Mutex::new(LruCache::new(NonZeroUsize::MIN)),
            ..GatewayUsage::default()
        })
    }

    /// The synchronization serialization `usage` holds for `tenant`.
    ///
    /// # Panics
    /// Panics when `tenant` is not retained.
    fn serial(usage: &GatewayUsage, tenant: DataTenantId) -> Arc<tokio::sync::Mutex<()>> {
        usage
            .tenants()
            .peek(&tenant)
            .map(|usage| Arc::clone(&usage.sync))
            .expect("tenant retained")
    }

    /// Handles to `tenant`'s serialization, counting the one this takes.
    fn handles(usage: &GatewayUsage, tenant: DataTenantId) -> usize {
        Arc::strong_count(&serial(usage, tenant))
    }

    /// Waits until a synchronization of `tenant` takes a handle to its
    /// serialization beyond the `before` already counted.
    ///
    /// # Panics
    /// Panics when none starts within ten seconds.
    async fn in_flight(usage: &GatewayUsage, tenant: DataTenantId, before: usize) {
        tokio::time::timeout(Duration::from_secs(10), async {
            while handles(usage, tenant) <= before {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the synchronization starts");
    }

    /// A synchronization waiting on or holding its tenant's serialization
    /// keeps that tenant from eviction even when it holds no usage, so
    /// capacity pressure refuses other tenants rather than replacing the
    /// serialization; completion and cancellation each release it.
    ///
    /// # Panics
    /// Panics when the fixture or a synchronization fails, the in-flight
    /// tenant is evicted or its serialization replaced, or a finished
    /// synchronization still pins its tenant.
    #[tokio::test]
    async fn in_flight_synchronization_pins_its_tenant() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let postgres = fixture.wyrd_postgres().clone();
        let tenant = fixture.data_tenant_id();
        let other = fixture
            .seed_additional_tenant("usage-eviction-other")
            .await
            .expect("other tenant seeds");
        let usage = one_tenant();
        usage.sync(&postgres, tenant).await.expect("first view");
        assert!(
            usage
                .tenants()
                .peek(&tenant)
                .is_some_and(TenantUsage::clean),
            "the tenant holds no usage"
        );
        let original = serial(&usage, tenant);

        let held = Arc::clone(&original).lock_owned().await;
        let before = handles(&usage, tenant);
        let waiting = tokio::spawn({
            let (usage, postgres) = (Arc::clone(&usage), postgres.clone());
            async move { usage.sync(&postgres, tenant).await }
        });
        in_flight(&usage, tenant, before).await;
        let refused = usage
            .sync(&postgres, other)
            .await
            .expect_err("an in-flight tenant is not evicted");
        assert_eq!(refused.code(), "WYRD_SERVER_503_SERVICE_UNAVAILABLE");
        let window = window_floor(Utc::now());
        assert!(usage.admit(other, window, &[], Vec::new()).is_err());
        assert!(Arc::ptr_eq(&serial(&usage, tenant), &original));
        drop(held);
        waiting
            .await
            .expect("synchronization joins")
            .expect("synchronization commits");
        drop(original);

        usage
            .sync(&postgres, other)
            .await
            .expect("a finished synchronization releases its tenant");
        let held = serial(&usage, other).lock_owned().await;
        let before = handles(&usage, other);
        let cancelled = tokio::spawn({
            let (usage, postgres) = (Arc::clone(&usage), postgres.clone());
            async move { usage.sync(&postgres, other).await }
        });
        in_flight(&usage, other, before).await;
        cancelled.abort();
        assert!(
            cancelled.await.is_err_and(|error| error.is_cancelled()),
            "the waiting synchronization is cancelled"
        );
        drop(held);
        usage
            .sync(&postgres, tenant)
            .await
            .expect("a cancelled synchronization releases its tenant");
    }
}
