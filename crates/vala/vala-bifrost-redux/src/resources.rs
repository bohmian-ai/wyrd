//! Portable pod resource detection and cross-role Bifrost admission.
//!
//! [`BifrostResourceGovernor`] is the single process-local authority for the
//! memory and disposable scratch resources shared by Scribe, Oracle, Forge,
//! and in-flight Bifrost transport bodies. It owns one shared Bifrost memory
//! cap — the detected process limit minus the configured server headroom,
//! optionally lowered by an operator — and one serialized total of held bytes.
//! Every fallible charge checks `held + new <= cap`; per-holder figures are
//! attribution for diagnostics only, so an idle role holds no share.
//! Detection uses only process-visible operating-system interfaces.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use datafusion::error::DataFusionError;
use datafusion::execution::memory_pool::MemoryLimit;
use datafusion::execution::memory_pool::{
    FairSpillPool, GreedyMemoryPool, MemoryConsumer, MemoryPool, MemoryReservation,
    TrackConsumersPool,
};
use num_traits::ToPrimitive;
use rustix::fs::statvfs;
use tokio::sync::Notify;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::QueryClass;

/// One mebibyte in bytes.
const MIB: usize = 1024 * 1024;
/// Default and smallest server memory headroom Bifrost never governs.
///
/// Headroom is accounting, not physically reserved RAM: server work outside
/// Bifrost may use any memory Bifrost has not consumed, and this figure only
/// bounds how high the shared Bifrost cap may rise.
pub const DEFAULT_SERVER_MEMORY_MIN_BYTES: usize = 1024 * MIB;
/// Filesystem free space that disposable query spill never consumes.
pub const MIN_SCRATCH_FREE_BYTES: u64 = 256 * MIB as u64;
/// Largest `DataFusion` memory ceiling any single Oracle query may be granted.
///
/// This is a *cap on a derived grant*, not a reservation. Admission debits no
/// memory at all; governed bytes are charged only as the query's `DataFusion`
/// consumers actually grow through the shared Oracle root, and this value bounds
/// how far one query may grow. Treating it as a reservation pinned node
/// concurrency at `budget / ceiling`, which let a single analytical query hold
/// 1.25 GiB to scan a handful of 5 KiB Parquet files and shed load at the lowest
/// production rung. See [`ORACLE_PARTITION_WORKING_MEMORY_BYTES`] for the
/// smaller quantum that sizes concurrency and partitions.
pub const ORACLE_PARTITION_MEMORY_BYTES: usize = 256 * MIB;
/// Memory one Oracle slot unit is *sized* against, in bytes.
///
/// This is a planning quantum, never a charge: no admission path debits it from
/// the shared Bifrost cap. It serves three distinct jobs that happen to want
/// the same number.
///
/// As a *sizing* input it is the working set one `DataFusion` execution
/// partition needs to make progress, so dividing the Oracle budget by it yields
/// how many slot units this pod can realistically run at once. As the *floor* of
/// [`BifrostResourceGovernor::oracle_memory_grant`] it is the smallest grant
/// that still lets a partition run at all. As a *partition-planning* input it
/// bounds how many partitions a granted envelope can feed.
///
/// The first governed memory charge happens later and elsewhere: when a query's
/// shared-pool consumer grows through [`GovernedMemoryRoot`].
///
/// It is deliberately far smaller than [`ORACLE_PARTITION_MEMORY_BYTES`], which
/// caps a whole query's memory envelope. Dividing a query envelope by the
/// envelope quantum always yields one partition, which silently serializes every
/// Interactive query onto a single core. Partition parallelism is bounded by CPU
/// and by available work; memory only clamps it downward once a query envelope
/// can no longer give each partition room to run.
pub const ORACLE_PARTITION_WORKING_MEMORY_BYTES: usize = 32 * MIB;
/// Fewest execution partitions any admitted Oracle query receives.
///
/// A single partition removes intra-query parallelism entirely, so even the
/// smallest query keeps two.
pub const ORACLE_MIN_TARGET_PARTITIONS: usize = 2;
/// Fixed Oracle footer-planning slot acquired before metadata I/O.
pub const ORACLE_METADATA_MEMORY_BYTES: usize = 40 * MIB;
/// Retry delays for exact-prefix scratch cleanup before fail-stop poisoning.
const SCRATCH_CLEANUP_BACKOFFS: [Duration; 3] = [
    Duration::from_millis(10),
    Duration::from_millis(50),
    Duration::from_millis(250),
];

/// Resolves how many Oracle slot units this node admits concurrently.
///
/// Concurrency is an explicit local capacity decision. It is taken from
/// [`ResourcePlan::oracle_query_slot_limit`] when the deployment configured one,
/// and otherwise from the tighter of this pod's memory and CPU bounds:
///
/// ```text
/// memory units = max(1, Bifrost cap / ORACLE_PARTITION_WORKING_MEMORY_BYTES)
/// CPU units    = max(1, 2 * effective CPU)
/// raw units    = min(memory units, CPU units)
/// ```
///
/// The memory divisor is [`ORACLE_PARTITION_WORKING_MEMORY_BYTES`] — the working
/// set one slot unit is sized for — not [`ORACLE_PARTITION_MEMORY_BYTES`], which
/// is the ceiling a query may grow into. Dividing by the ceiling made
/// concurrency a side effect of per-query generosity: raising the ceiling so one
/// query could use more memory silently reduced how many queries the node would
/// accept. Neither value is debited at admission; a slot unit is concurrency
/// ownership, and governed memory is charged only as a query's pools grow.
///
/// The CPU term restores the documented two-units-per-core default. A slot unit
/// is an admission unit rather than a thread, but a pod that admits far more
/// concurrent queries than it has cores to run them converts latency into queue
/// time inside execution, where no admission signal can see it. Deployments that
/// want a different ratio set an explicit limit.
///
/// Effective CPU is the sole CPU source; there is no separate configured core
/// count to disagree with it.
///
/// This value sizes both leader admission and follower participation, which
/// matters because one node is usually both: it leads its own queries while
/// serving fragments for queries other nodes lead.
///
/// # Errors
///
/// Returns an invalid-plan error when effective CPU is zero, and an overflow
/// error when checked Oracle capacity arithmetic cannot complete.
pub fn oracle_worker_slots(plan: ResourcePlan) -> Result<usize, BifrostResourceError> {
    if let Some(configured) = plan.oracle_query_slot_limit {
        return Ok(configured.max(1));
    }
    if plan.effective_cpu == 0 {
        return Err(BifrostResourceError::InvalidPlan {
            detail: "Oracle slot derivation requires positive effective CPU".to_owned(),
        });
    }
    let memory_units = (plan.managed_memory_bytes / ORACLE_PARTITION_WORKING_MEMORY_BYTES).max(1);
    let cpu_units = plan
        .effective_cpu
        .checked_mul(ORACLE_SLOT_UNITS_PER_CPU)
        .ok_or_else(accounting_overflow)?
        .max(1);
    Ok(memory_units.min(cpu_units))
}

/// Slot units one effective CPU contributes to derived Oracle concurrency.
const ORACLE_SLOT_UNITS_PER_CPU: usize = 2;

/// Immutable pod-local split of Oracle slot units between the two classes.
///
/// The split is the whole local capacity contract:
///
/// ```text
/// total units             = interactive_floor_units + analytical_max_units
/// protected Interactive   = interactive_floor_units
/// maximum Analytical      = analytical_max_units
/// maximum Interactive     = total units
/// ```
///
/// Analytical work can never enter the protected floor, while Interactive work
/// may borrow whatever Analytical is not using. `analytical_max_units` of zero
/// is a valid state on a pod too small to run one two-unit Analytical query; its
/// Analytical admission is refused immediately rather than queued forever.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OracleClassSplit {
    /// Slot units reserved for Interactive work alone.
    pub interactive_floor_units: u32,
    /// Largest number of slot units Analytical work may hold at once.
    pub analytical_max_units: u32,
}

impl OracleClassSplit {
    /// Derives the default split from a pod's raw local slot units.
    ///
    /// One unit is preserved for Interactive and the remainder becomes the
    /// Analytical maximum. A remainder below one Analytical query's two-unit
    /// cost is folded back into the Interactive floor, because exposing a class
    /// that can never admit a single query is worse than not exposing it.
    #[must_use]
    pub fn derive(raw_units: u32) -> Self {
        let raw_units = raw_units.max(1);
        let analytical = raw_units.saturating_sub(1);
        if analytical < ANALYTICAL_QUERY_SLOT_UNITS {
            return Self {
                interactive_floor_units: raw_units,
                analytical_max_units: 0,
            };
        }
        Self {
            interactive_floor_units: 1,
            analytical_max_units: analytical,
        }
    }

    /// Returns the total slot units this pod admits across both classes.
    #[must_use]
    pub const fn total_units(self) -> u32 {
        self.interactive_floor_units
            .saturating_add(self.analytical_max_units)
    }

    /// Reports whether this pod can ever admit one Analytical query.
    #[must_use]
    pub const fn admits_analytical(self) -> bool {
        self.analytical_max_units >= ANALYTICAL_QUERY_SLOT_UNITS
    }
}

/// Slot units one Analytical query or worker occupies.
pub const ANALYTICAL_QUERY_SLOT_UNITS: u32 = 2;

/// Bifrost roles whose capabilities a composition issues.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BifrostRole {
    /// Durable ingest, WAL, and persistence work.
    Scribe,
    /// Query planning and execution.
    Oracle,
    /// Background compaction and rewrite work.
    Forge,
}

/// Source that supplied one resolved portable resource bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceSource {
    /// Explicit absolute operator override.
    Override,
    /// Linux cgroup version 2 controller.
    CgroupV2,
    /// Linux cgroup version 1 controller.
    CgroupV1,
    /// Host or process-visible operating-system fallback.
    Host,
    /// Filesystem containing the disposable scratch root.
    Filesystem,
    /// Deterministic injected test snapshot.
    Injected,
}

/// Optional absolute caps and active roles used to derive one resource plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BifrostResourcePolicy {
    /// Roles activated in this process.
    pub roles: BTreeSet<BifrostRole>,
    /// Optional server headroom; `None` selects
    /// [`DEFAULT_SERVER_MEMORY_MIN_BYTES`], and a value below it is refused.
    pub server_memory_min_bytes: Option<usize>,
    /// Optional lower shared Bifrost memory cap.
    ///
    /// It may only lower the cap below `process limit - server minimum`; a
    /// value above that is an impossible plan and refuses boot.
    pub bifrost_memory_limit_bytes: Option<usize>,
    /// Optional absolute disposable scratch cap; it may only reduce detection.
    pub scratch_limit_bytes: Option<u64>,
    /// Optional effective CPU cap; it may only reduce detection.
    pub effective_cpu: Option<usize>,
    /// Optional explicit Oracle query slot-unit concurrency limit.
    ///
    /// `None` derives the limit from this pod's own memory and CPU through
    /// [`oracle_worker_slots`]. Unlike the memory and CPU caps this one may
    /// raise as well as reduce the derived value: it is a deliberate capacity
    /// decision by the deployment, not a detected process bound.
    pub oracle_query_slot_limit: Option<usize>,
    /// Existing server-composed Oracle scratch root.
    pub scratch_root: PathBuf,
    /// Optional explicit physical roots registered together during live boot.
    pub volume_roots: Option<BifrostVolumeRoots>,
}

/// Immutable process-visible inputs used to calculate a resource plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemResourceSnapshot {
    /// Tightest positive process-visible memory bound.
    pub memory_limit_bytes: usize,
    /// Tightest positive process-visible CPU bound.
    pub effective_cpu: usize,
    /// Total capacity of the scratch filesystem.
    pub scratch_capacity_bytes: u64,
    /// Currently available bytes on the scratch filesystem.
    pub scratch_available_bytes: u64,
    /// Source of the resolved memory bound.
    pub memory_source: ResourceSource,
    /// Source of the resolved CPU bound.
    pub cpu_source: ResourceSource,
}

/// Explicit physical roots registered once during Bifrost boot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BifrostVolumeRoots {
    /// Durable Scribe WAL base.
    pub wal: PathBuf,
    /// Durable Scribe staged-member base.
    ///
    /// This namespace is never cleared at boot: the records under it are what
    /// authorized retiring the WAL segments behind their rows.
    pub scribe_stage: PathBuf,
    /// Process-owned Scribe output scratch namespace.
    pub scribe_output_scratch: PathBuf,
}

/// Publishes one closed role-memory lifecycle transition.
fn record_memory_transition(role: &'static str, result: &'static str, current_bytes: usize) {
    metrics::counter!(
        "bifrost_resource_acquisitions_total",
        "role" => role,
        "resource" => "memory",
        "result" => result
    )
    .increment(1);
    metrics::gauge!(
        "bifrost_resource_current_bytes",
        "role" => role,
        "resource" => "memory"
    )
    .set(current_bytes.to_f64().unwrap_or(f64::MAX));
}

/// Emits the four pod-local Oracle capacity gauges from one locked ledger read.
///
/// `memory_limit` is the governed Oracle budget, `memory_used` is Oracle query
/// memory (see [`record_oracle_memory`]), and the slot series are the aggregate
/// slot ledger's limit and occupancy. Labels come from a closed `kind` domain
/// and never carry tenant, query, node, or table identity, and nothing here
/// claims a cluster-wide quota. It is called wherever an Oracle owner is
/// admitted or released, so an Oracle-only pod that never runs the Scribe tick
/// still exports live occupancy. Per-growth reservations and releases call
/// only [`record_oracle_memory`], because the limit and slot series cannot
/// change there.
fn record_oracle_capacity(state: &ResourceState, plan: &ResourcePlan, split: OracleClassSplit) {
    metrics::gauge!("bifrost_oracle_local_bytes", "kind" => "memory_limit")
        .set(plan.managed_memory_bytes.to_f64().unwrap_or(f64::MAX));
    record_oracle_memory(state);
    metrics::gauge!("bifrost_oracle_local_slot_units", "kind" => "limit")
        .set(f64::from(split.total_units()));
    metrics::gauge!("bifrost_oracle_local_slot_units", "kind" => "used")
        .set(f64::from(state.oracle_query_slot_units));
}

/// Emits `memory_used`, the one capacity series a query's growth changes.
///
/// The value is Oracle query memory, not pod RAM: governed query bytes plus
/// the infallible bytes `DataFusion` grew past what the root could govern,
/// both read from the same locked state. Because the infallible share is real
/// memory the process holds, `memory_used` may exceed `memory_limit`.
fn record_oracle_memory(state: &ResourceState) {
    metrics::gauge!("bifrost_oracle_local_bytes", "kind" => "memory_used").set(
        state
            .oracle_query_memory_used_bytes
            .saturating_add(state.infallible_headroom_bytes)
            .to_f64()
            .unwrap_or(f64::MAX),
    );
}

/// Name prefix of the scratch directory one assembly claim merges into.
const CLAIM_SCRATCH_PREFIX: &str = "scribe-claim-";

/// Reports whether one directory name is a scratch directory this capability
/// created and may therefore delete.
///
/// Deletion is gated on the same prefixes creation stamps, so a directory this
/// process does not own — anything else sharing the registered namespace — is
/// never a cleanup or reconciliation target.
fn is_owned_scratch_name(name: &str) -> bool {
    name.starts_with(CLAIM_SCRATCH_PREFIX)
}

/// Clears only process-owned Scribe runtime directories before readiness.
///
/// # Errors
///
/// Returns unavailable when the registered namespace cannot be listed or an
/// exact runtime child cannot be removed and parent-directory fsync confirmed.
fn reconcile_scribe_scratch_namespace(root: &Path) -> Result<(), BifrostResourceError> {
    let entries = fs::read_dir(root).map_err(|error| BifrostResourceError::Unavailable {
        detail: format!("cannot reconcile Scribe scratch namespace: {error}"),
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| BifrostResourceError::Unavailable {
            detail: format!("cannot inspect Scribe scratch namespace entry: {error}"),
        })?;
        let name = entry.file_name();
        if name.to_str().is_some_and(is_owned_scratch_name) {
            retry_scratch_cleanup(|| remove_exact_scratch_prefix(root, &entry.path())).map_err(
                |error| BifrostResourceError::Unavailable {
                    detail: format!("cannot remove retained Scribe scratch namespace: {error}"),
                },
            )?;
        }
    }
    Ok(())
}

/// Scribe output scratch namespace where assembly claims write their Parquet.
///
/// Not capacity-governed: a full device surfaces as the writer's own IO error.
/// Registration clears only claim directories this process family created, so
/// residue from an interrupted claim never outlives a restart.
#[derive(Debug, Clone)]
pub struct ScratchVolume {
    /// Registered Scribe output namespace.
    root: PathBuf,
}

impl ScratchVolume {
    /// Clears retained claim directories and binds the namespace.
    ///
    /// # Errors
    ///
    /// Returns unavailable when the namespace cannot be listed or a retained
    /// claim directory cannot be removed and its parent fsynced.
    pub fn register(root: PathBuf) -> Result<Self, BifrostResourceError> {
        reconcile_scribe_scratch_namespace(&root)?;
        Ok(Self { root })
    }

    /// Creates the exact owned output directory one assembly claim merges into.
    ///
    /// A claim spans several shard generations, so its directory is named after
    /// the claim rather than after any one of them; restart reconciliation then
    /// attributes surviving residue to the publication that was interrupted.
    ///
    /// # Errors
    ///
    /// Returns a typed plan error for an unsafe stream or claim component, or
    /// an unavailable error when the exact owned directory cannot be created.
    pub fn create_scribe_claim(
        &self,
        stream: &str,
        claim: &str,
    ) -> Result<ScribeClaimScratch, BifrostResourceError> {
        let component = safe_scratch_component(stream)?;
        let claim = safe_scratch_component(claim)?;
        let suffix = SCRATCH_NAMESPACE_SEQUENCE.fetch_add(1, AtomicOrdering::Relaxed);
        let path = self.root.join(format!(
            "{CLAIM_SCRATCH_PREFIX}{component}-{claim}-{suffix}"
        ));
        fs::create_dir(&path).map_err(|error| BifrostResourceError::Unavailable {
            detail: format!("cannot create Scribe claim scratch: {error}"),
        })?;
        Ok(ScribeClaimScratch {
            path,
            namespace_root: self.root.clone(),
        })
    }
}

/// Process-local suffix preventing generation-directory collisions.
static SCRATCH_NAMESPACE_SEQUENCE: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Validates a stream identity for use as one filesystem path component.
///
/// # Errors
///
/// Returns an invalid-plan error when the identity is empty or contains a
/// character outside the stable ASCII identifier alphabet.
fn safe_scratch_component(stream: &str) -> Result<&str, BifrostResourceError> {
    if stream.is_empty()
        || !stream
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(BifrostResourceError::InvalidPlan {
            detail: "Scribe scratch stream identity is not a safe path component".to_owned(),
        });
    }
    Ok(stream)
}

/// Claim-owned Scribe output directory.
///
/// Dropping it without [`Self::cleanup`] leaves the directory for the next
/// registration's reconciliation to remove.
#[derive(Debug)]
pub struct ScribeClaimScratch {
    /// Exact directory created for this claim.
    path: PathBuf,
    /// Registered parent used to prove cleanup containment and fsync completion.
    namespace_root: PathBuf,
}

impl ScribeClaimScratch {
    /// Returns the exact directory available to the claim writer.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Removes the exact claim directory with bounded retries.
    ///
    /// # Errors
    ///
    /// Returns unavailable after all bounded cleanup attempts fail; the
    /// directory is then left for restart reconciliation.
    pub fn cleanup(self) -> Result<(), BifrostResourceError> {
        retry_scratch_cleanup(|| remove_exact_scratch_prefix(&self.namespace_root, &self.path))
            .map_err(|error| BifrostResourceError::Unavailable {
                detail: format!("Scribe claim scratch cleanup exhausted bounded retries: {error}"),
            })
    }
}

/// Retries one exact cleanup operation at the constitution's bounded delays.
fn retry_scratch_cleanup(mut cleanup: impl FnMut() -> std::io::Result<()>) -> std::io::Result<()> {
    let mut final_error = match cleanup() {
        Ok(()) => return Ok(()),
        Err(error) => error,
    };
    for delay in SCRATCH_CLEANUP_BACKOFFS {
        std::thread::sleep(delay);
        match cleanup() {
            Ok(()) => return Ok(()),
            Err(error) => final_error = error,
        }
    }
    Err(final_error)
}

/// Deletes one proven child directory and fsyncs its registered parent.
fn remove_exact_scratch_prefix(root: &Path, owned: &Path) -> std::io::Result<()> {
    if owned.parent() != Some(root)
        || !owned
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_owned_scratch_name)
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "scratch cleanup target escaped its registered namespace",
        ));
    }
    match fs::remove_dir_all(owned) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::File::open(root)?.sync_all()
}

/// Exact immutable capacity calculation shared by boot, telemetry, and tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourcePlan {
    /// Detected process memory limit.
    pub memory_limit_bytes: usize,
    /// Effective CPU capacity used for query parallelism.
    pub effective_cpu: usize,
    /// Configured Oracle query slot-unit limit, or `None` to derive from CPU.
    ///
    /// Resolved through [`oracle_worker_slots`] rather than read directly, so
    /// the configured and derived paths cannot diverge.
    pub oracle_query_slot_limit: Option<usize>,
    /// Server headroom the Bifrost cap leaves below the process limit.
    ///
    /// Accounting only: server work has no application cap and may use any
    /// memory Bifrost has not consumed.
    pub server_memory_min_bytes: usize,
    /// The one shared Bifrost memory cap every governed holder charges.
    pub managed_memory_bytes: usize,
    /// Whether this process composes a Scribe capability.
    pub scribe_enabled: bool,
    /// Whether this process composes an Oracle capability.
    pub oracle_enabled: bool,
    /// Whether this process composes a Forge capability.
    pub forge_enabled: bool,
    /// Disposable scratch bytes available after the filesystem floor.
    pub scratch_limit_bytes: u64,
}

/// Portable source selected for each resolved resource dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedResourceSources {
    /// Source of the memory bound after applying any reducing override.
    pub memory: ResourceSource,
    /// Source of the effective CPU bound after applying any reducing override.
    pub cpu: ResourceSource,
    /// Source of the disposable scratch bound after applying any reducing override.
    pub scratch: ResourceSource,
}

/// Typed failure from portable resource detection or checked accounting.
#[derive(Debug, thiserror::Error)]
pub enum BifrostResourceError {
    /// No safe positive bound could be resolved for a required resource.
    #[error("Bifrost resource is unavailable: {detail}")]
    Unavailable {
        /// Bounded operator-facing reason.
        detail: String,
    },
    /// A configured or derived resource plan violates the minimum floors.
    #[error("Bifrost resource plan is invalid: {detail}")]
    InvalidPlan {
        /// Exact failed calculation.
        detail: String,
    },
    /// A live request cannot fit without violating another owner's capacity.
    #[error("Bifrost resource capacity is occupied: {detail}")]
    Occupied {
        /// Exact bounded capacity reason.
        detail: String,
    },
    /// Shared accounting no longer has a trustworthy ownership state.
    #[error("Bifrost resource accounting is poisoned: {detail}")]
    Poisoned {
        /// First observed accounting failure.
        detail: String,
    },
}

/// Closed first-failure reason for process-wide resource health.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BifrostResourcePoisonReason {
    /// Exact counter ownership diverged or underflowed.
    Accounting = 1,
    /// A physical-volume observation contradicted retained ownership.
    Volume = 2,
    /// An invariant-bearing runtime owner survived its finalizer.
    RuntimeOwner = 3,
}

/// Cloneable process lifecycle signal shared by synchronous RAII owners.
#[derive(Debug, Clone)]
pub struct BifrostResourceHealth {
    /// Zero while healthy, otherwise the first closed poison reason.
    state: Arc<AtomicU8>,
    /// Wakeup for the single supervised lifecycle future.
    notify: Arc<Notify>,
}

impl Default for BifrostResourceHealth {
    fn default() -> Self {
        Self {
            state: Arc::new(AtomicU8::new(0)),
            notify: Arc::new(Notify::new()),
        }
    }
}

impl BifrostResourceHealth {
    /// Records only the first poison reason and wakes the process supervisor.
    pub fn poison(&self, reason: BifrostResourcePoisonReason) {
        if self
            .state
            .compare_exchange(
                0,
                reason as u8,
                AtomicOrdering::AcqRel,
                AtomicOrdering::Acquire,
            )
            .is_ok()
        {
            let reason_label = match reason {
                BifrostResourcePoisonReason::Accounting => "accounting",
                BifrostResourcePoisonReason::Volume => "volume",
                BifrostResourcePoisonReason::RuntimeOwner => "runtime_owner",
            };
            metrics::counter!(
                "bifrost_resource_poisons_total",
                "resource" => "root",
                "reason" => reason_label
            )
            .increment(1);
            // ERROR because this is terminal: the supervised health watcher
            // observes it and ends the serving process. A metric alone leaves
            // an operator with a server that stopped and no log saying why.
            tracing::error!(
                reason = reason_label,
                "Bifrost resource accounting poisoned; this wyrd-server process will terminate"
            );
            self.notify.notify_waiters();
        }
    }

    /// Returns the first closed poison reason, or `None` while healthy.
    #[must_use]
    pub fn reason(&self) -> Option<BifrostResourcePoisonReason> {
        match self.state.load(AtomicOrdering::Acquire) {
            1 => Some(BifrostResourcePoisonReason::Accounting),
            2 => Some(BifrostResourcePoisonReason::Volume),
            3 => Some(BifrostResourcePoisonReason::RuntimeOwner),
            _ => None,
        }
    }

    /// Waits until the first poison and returns its stable lifecycle error.
    ///
    /// # Errors
    ///
    /// Always returns [`BifrostResourceError::Poisoned`] after notification;
    /// the async result shape lets application supervision treat it as a
    /// terminal role future without a second signaling mechanism.
    pub async fn wait_for_poison(&self) -> Result<(), BifrostResourceError> {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            // Register before sampling the reason. `notify_waiters` reaches only
            // already-registered waiters and `notified()` does not register until
            // first polled, so a poison published between the sample and the await
            // would otherwise be dropped and this watcher would sleep through the
            // fail-closed signal it exists to observe.
            notified.as_mut().enable();
            if let Some(reason) = self.reason() {
                return Err(BifrostResourceError::Poisoned {
                    detail: format!("resource health entered {reason:?}"),
                });
            }
            notified.await;
        }
    }
}

/// Point-in-time live resource ownership.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceSnapshot {
    /// Immutable boot calculation.
    pub plan: ResourcePlan,
    /// Every byte currently charged against the shared cap, including
    /// infallible headroom.
    pub governed_memory_used_bytes: usize,
    /// Bytes Scribe currently holds (attribution only).
    pub scribe_memory_used_bytes: usize,
    /// Bytes Oracle currently holds (attribution only).
    pub oracle_memory_used_bytes: usize,
    /// Bytes Forge rewrites currently hold (attribution only).
    pub forge_memory_used_bytes: usize,
    /// Encoded transport-body bytes currently held (attribution only).
    pub transport_memory_used_bytes: usize,
    /// Aggregate number of live Oracle query owners.
    pub oracle_active_queries: u32,
    /// Live interactive Oracle query owners.
    pub oracle_interactive_queries: u32,
    /// Live analytical Oracle query owners.
    pub oracle_analytical_queries: u32,
    /// Aggregate slot units retained by Oracle query owners.
    pub oracle_query_slot_units: u32,
    /// Aggregate memory retained specifically by Oracle query owners.
    pub oracle_query_memory_used_bytes: usize,
    /// Bytes `DataFusion`'s infallible growth path holds above the shared cap.
    ///
    /// These are real, retained bytes accounted as server headroom rather than
    /// governed capacity. They count toward the held total, so they suppress
    /// later fallible growth until the owning consumer shrinks or drops.
    pub infallible_headroom_bytes: usize,
    /// Whether at least one Oracle query owner is active.
    pub oracle_query_active: bool,
}

/// Closed Scribe lifecycle attribution attached to one root-owned memory lease.
pub(crate) type ScribeMemoryCategory = crate::scribe::memory::MemoryCategory;

/// One checked Scribe memory request admitted by the process root.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ScribeMemoryRequest {
    /// Exact bytes that become owned on successful admission.
    pub bytes: usize,
    /// Lifecycle category charged by this owner.
    pub category: ScribeMemoryCategory,
    /// Optional bounded shard attribution.
    pub shard: Option<usize>,
}

/// Closed Scribe attribution exported only for production-equivalent tests.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceAttributionSnapshot {
    /// Exact category totals indexed by [`ScribeMemoryCategory`] discriminant.
    pub category_bytes: [usize; crate::scribe::memory::MEMORY_CATEGORY_COUNT],
    /// Exact shard totals for shards that currently own bytes.
    pub shard_bytes: BTreeMap<usize, usize>,
}

/// One complete Oracle query request.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OracleResourceRequest {
    /// Scheduling class whose protected-capacity rules apply to the query.
    pub query_class: QueryClass,
    /// Exact positive class memory quantum, validated but never debited.
    ///
    /// Admission checks this against the class definition so a hand-built
    /// request cannot disagree with it; it reserves no bytes. Governed memory is
    /// charged only when the query's shared-pool consumers grow.
    pub memory_bytes: usize,
    /// Class spill quantum: the most `DataFusion` may write to disk for this
    /// query, clamped to the resolved scratch limit. A limit only; admission
    /// never debits it.
    pub spill_limit_bytes: u64,
    /// Exact positive Oracle slot-unit demand.
    pub slot_units: u32,
    /// Fraction of pinned input bytes expected to be local, in `[0, 1]`.
    pub local_ratio: f64,
}

impl OracleResourceRequest {
    /// Builds the exact admission demand one query of `query_class` must present.
    ///
    /// This is the single definition of a class quantum. Admission validates an
    /// incoming request against it, so a caller that restates the quantum by hand
    /// and a later change to the charge cannot silently disagree — the request is
    /// simply refused. Every production caller builds its request here.
    ///
    /// The memory term is a validated sizing quantum, not a reservation: no
    /// admission path debits it, and the ceiling the query may actually grow
    /// into is derived per query and is generally much larger. The spill term
    /// is likewise only a ceiling: an unspilled query owns no disk.
    #[must_use]
    pub fn for_class(query_class: QueryClass, local_ratio: f64) -> Self {
        let slot_units = match query_class {
            QueryClass::Interactive => 1,
            QueryClass::Analytical => 2,
        };
        Self {
            query_class,
            memory_bytes: ORACLE_PARTITION_WORKING_MEMORY_BYTES * slot_units as usize,
            spill_limit_bytes: ORACLE_PARTITION_MEMORY_BYTES as u64 * u64::from(slot_units),
            slot_units,
            local_ratio,
        }
    }
}

/// Closed remote Oracle worker sizes admitted atomically by the target root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OracleWorkerClass {
    /// One slot unit for ordinary fragment execution.
    Interactive,
    /// Two slot units for analytical fragment execution.
    Analytical,
}

impl OracleWorkerClass {
    /// Returns the scheduling class this worker charges the shared slot ledger under.
    const fn query_class(self) -> QueryClass {
        match self {
            Self::Interactive => QueryClass::Interactive,
            Self::Analytical => QueryClass::Analytical,
        }
    }

    /// Returns the slot units one worker of this class occupies.
    fn slot_units(self) -> u32 {
        match self {
            Self::Interactive => 1,
            Self::Analytical => 2,
        }
    }
}

/// Closed holders whose bytes the shared cap attributes for diagnostics.
///
/// Attribution never partitions capacity: every holder charges the same total.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemoryHolder {
    /// Scribe ingress, memtable, staged, and follower leases.
    Scribe = 0,
    /// Oracle metadata owners and query `DataFusion` consumers.
    Oracle = 1,
    /// Forge rewrite `DataFusion` consumers.
    Forge = 2,
    /// Encoded HTTP and gRPC transport bodies.
    Transport = 3,
}

/// Number of [`MemoryHolder`] attribution slots.
const MEMORY_HOLDER_COUNT: usize = 4;

impl MemoryHolder {
    /// Returns the closed metric label for this holder.
    const fn label(self) -> &'static str {
        match self {
            Self::Scribe => "scribe",
            Self::Oracle => "oracle",
            Self::Forge => "forge",
            Self::Transport => "transport",
        }
    }
}

#[derive(Debug, Default)]
struct ResourceState {
    /// Held bytes attributed to each [`MemoryHolder`].
    held_bytes: [usize; MEMORY_HOLDER_COUNT],
    /// Infallible `DataFusion` overshoot above the cap, released on shrink.
    infallible_headroom_bytes: usize,
    oracle_active_queries: u32,
    oracle_interactive_queries: u32,
    oracle_analytical_queries: u32,
    oracle_query_slot_units: u32,
    oracle_analytical_slot_units: u32,
    oracle_query_memory_used_bytes: usize,
    scribe_category_bytes: [usize; crate::scribe::memory::MEMORY_CATEGORY_COUNT],
    scribe_shard_bytes: BTreeMap<usize, usize>,
    memory_epoch: u64,
    /// Advances only when Oracle slot units return to the ledger.
    ///
    /// Oracle admission refuses on slots alone, so its waiters
    /// follow this epoch rather than `memory_epoch`, which also moves on every
    /// query-memory grow and shrink and would wake the whole queue per batch.
    oracle_capacity_epoch: u64,
    poisoned: bool,
}

impl ResourceState {
    /// Returns bytes held by one holder.
    const fn held(&self, holder: MemoryHolder) -> usize {
        self.held_bytes[holder as usize]
    }

    /// Returns every byte charged against the shared cap, headroom included.
    ///
    /// Each component is individually bounded by the cap or by recorded
    /// infallible growth, so the saturating sum only saturates on a ledger the
    /// reconciliation check already rejects.
    fn governed_total(&self) -> usize {
        self.held_bytes
            .iter()
            .fold(self.infallible_headroom_bytes, |total, bytes| {
                total.saturating_add(*bytes)
            })
    }
}

/// Single concrete process-local owner for Bifrost memory and scratch.
///
/// This root ledger is crate-private by design. External production and
/// production-equivalent callers reach it only through the narrow role
/// capabilities issued by [`BifrostRuntimeResources::compose_roles`], so no
/// caller outside this module can construct or clone a sibling root.
#[derive(Debug, Clone)]
pub(crate) struct BifrostResourceGovernor {
    inner: Arc<ResourceGovernorInner>,
}

/// Complete process-local Bifrost resource composition.
///
/// This is the sole production-equivalent bootstrap for Scribe, Oracle, and
/// Forge. Live detection and deterministic test observations converge here so
/// every role derives its accounting handles from one root governor.
#[derive(Debug, Clone)]
pub struct BifrostRuntimeResources {
    /// Root authority for process memory and disposable scratch ownership.
    governor: BifrostResourceGovernor,
    /// Process-wide encoded transport-body admission charging the shared cap.
    transport: crate::gate::limits::BifrostTransportAdmission,
    /// Registered Scribe stage and output roots, present on live boot.
    volumes: Option<BifrostVolumeRoots>,
}

impl BifrostRuntimeResources {
    /// Returns the sole process resource-health lifecycle signal.
    #[must_use]
    pub fn health(&self) -> BifrostResourceHealth {
        self.governor.inner.health.clone()
    }
    /// Detects process-visible resources and constructs the shared role graph.
    ///
    /// The transport message maximum is the default ingest request ceiling.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError`] when detection or checked root-policy
    /// validation fails.
    pub fn detect(policy: BifrostResourcePolicy) -> Result<Self, BifrostResourceError> {
        Self::detect_with_transport_message_limit(
            policy,
            crate::gate::limits::BIFROST_INGEST_REQUEST_LIMIT_BYTES,
        )
    }

    /// Detects resources while freezing an operator-selected transport message maximum.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError`] when detection, root-policy validation,
    /// or the selected message-to-aggregate relationship is invalid.
    pub fn detect_with_transport_message_limit(
        policy: BifrostResourcePolicy,
        transport_message_limit_bytes: usize,
    ) -> Result<Self, BifrostResourceError> {
        let fallback_memory = policy.bifrost_memory_limit_bytes.map(|cap| {
            cap.saturating_add(
                policy
                    .server_memory_min_bytes
                    .unwrap_or(DEFAULT_SERVER_MEMORY_MIN_BYTES),
            )
        });
        let snapshot = detect_snapshot(&policy.scratch_root, fallback_memory)?;
        Self::from_snapshot_with_transport_message_limit(
            snapshot,
            policy,
            transport_message_limit_bytes,
        )
    }

    /// Constructs the shared role graph from a complete resource observation.
    ///
    /// The injected path runs exactly the same policy and role-composition
    /// stage as live boot; callers may vary observations but cannot request a
    /// derived grant or construct a sibling governor.
    ///
    /// The transport message maximum is the default ingest request ceiling.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError`] when the observation cannot satisfy the
    /// checked root policy.
    pub fn from_snapshot(
        snapshot: SystemResourceSnapshot,
        policy: BifrostResourcePolicy,
    ) -> Result<Self, BifrostResourceError> {
        Self::from_snapshot_with_transport_message_limit(
            snapshot,
            policy,
            crate::gate::limits::BIFROST_INGEST_REQUEST_LIMIT_BYTES,
        )
    }

    /// Constructs the shared graph with an operator-selected message maximum.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError`] when the observation, root policy, or
    /// selected message-to-aggregate relationship is invalid.
    pub fn from_snapshot_with_transport_message_limit(
        snapshot: SystemResourceSnapshot,
        policy: BifrostResourcePolicy,
        transport_message_limit_bytes: usize,
    ) -> Result<Self, BifrostResourceError> {
        let volumes = policy.volume_roots.clone();
        if let Some(roots) = &volumes {
            ScratchVolume::register(roots.scribe_output_scratch.clone())?;
        }
        let root = BifrostResourceGovernor::from_snapshot(snapshot, policy)?;
        let transport = crate::gate::limits::BifrostTransportAdmission::new(
            root.clone(),
            transport_message_limit_bytes,
        )?;
        Ok(Self {
            transport,
            governor: root,
            volumes,
        })
    }

    /// Composes role capabilities from one deterministic injected observation.
    ///
    /// Inline unit tests elsewhere in this crate need a live capability without
    /// restating a policy literal. This helper only supplies raw observations:
    /// it runs the same [`Self::from_snapshot`] policy stage and the same
    /// [`Self::compose_roles`] operation production boot runs, so no test can
    /// request a derived grant or build a sibling root through it.
    ///
    /// `memory_limit_bytes` is the shared Bifrost cap the test wants; the
    /// injected process observation adds the default server headroom to it.
    ///
    /// # Panics
    ///
    /// Panics when the supplied observation cannot satisfy the policy, which in
    /// a unit test is an authoring error rather than a runtime condition.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub(crate) fn composed_for_test(
        memory_limit_bytes: usize,
        scratch_limit_bytes: u64,
        roles: impl IntoIterator<Item = BifrostRole>,
    ) -> BifrostRoleResources {
        let roles: BTreeSet<BifrostRole> = roles.into_iter().collect();
        let scratch_available_bytes = scratch_limit_bytes
            .checked_add(MIN_SCRATCH_FREE_BYTES)
            .expect("injected scratch observation must not overflow");
        Self::from_snapshot(
            SystemResourceSnapshot {
                memory_limit_bytes: memory_limit_bytes
                    .checked_add(DEFAULT_SERVER_MEMORY_MIN_BYTES)
                    .expect("injected memory observation must not overflow"),
                effective_cpu: 4,
                scratch_capacity_bytes: scratch_available_bytes,
                scratch_available_bytes,
                memory_source: ResourceSource::Injected,
                cpu_source: ResourceSource::Injected,
            },
            BifrostResourcePolicy {
                roles,
                server_memory_min_bytes: None,
                bifrost_memory_limit_bytes: None,
                scratch_limit_bytes: Some(scratch_limit_bytes),
                effective_cpu: None,
                oracle_query_slot_limit: None,
                scratch_root: PathBuf::new(),
                volume_roots: None,
            },
        )
        .expect("injected observation must satisfy the resource policy")
        .compose_roles()
        .expect("composed roles must be issued from an unpoisoned root")
    }

    /// Issues every enabled role capability from this one retained root.
    ///
    /// This is the sole composition operation for live boot, test servers,
    /// cluster nodes, journeys, Forge fixtures, and benchmarks. Callers select
    /// roles, paths, and observations; they never derive a grant, clone the
    /// root ledger, or build a generic process-equivalent pool.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Poisoned`] when the root ledger cannot
    /// be inspected while determining which capabilities are enabled.
    pub fn compose_roles(&self) -> Result<BifrostRoleResources, BifrostResourceError> {
        let plan = self.governor.plan();
        let scribe = plan.scribe_enabled.then(|| ScribeResources {
            governor: self.governor.clone(),
            volumes: self.volumes.clone(),
            follower_permits: Arc::new(Semaphore::new(plan.effective_cpu.max(1))),
        });
        Ok(BifrostRoleResources {
            scribe,
            oracle: plan.oracle_enabled.then(|| OracleResources {
                governor: self.governor.clone(),
                memory_root: Arc::new(GovernedMemoryRoot::new(
                    self.governor.clone(),
                    MemoryHolder::Oracle,
                )),
                #[cfg(feature = "test-support")]
                memory_hold: Arc::new(OracleQueryMemoryHold::default()),
            }),
            forge: plan.forge_enabled.then(|| ForgeResources {
                memory_root: Arc::new(GovernedMemoryRoot::new(
                    self.governor.clone(),
                    MemoryHolder::Forge,
                )),
            }),
            governor: self.governor.clone(),
            transport: self.transport.clone(),
        })
    }

    /// Returns the immutable checked resource plan.
    #[must_use]
    pub fn plan(&self) -> ResourcePlan {
        self.governor.plan()
    }

    /// Returns the resolved source for every resource dimension.
    #[must_use]
    pub fn sources(&self) -> ResolvedResourceSources {
        self.governor.sources()
    }

    /// Reports whether a composition references this exact process owner.
    ///
    /// This supports lifecycle inspection without exposing the allocator's
    /// internal synchronization or permitting construction of a second root.
    #[must_use]
    pub fn shares_root_with(&self, other: &BifrostRoleResources) -> bool {
        Arc::ptr_eq(&self.governor.inner, &other.governor.inner)
    }
}

/// Enabled narrow role capabilities issued from one composition operation.
///
/// The result retains only the capabilities the checked policy activated. It
/// deliberately exposes no raw-root accessor and no generic pool factory, so a
/// production-equivalent caller cannot construct or clone a sibling root.
#[derive(Debug, Clone)]
pub struct BifrostRoleResources {
    scribe: Option<ScribeResources>,
    oracle: Option<OracleResources>,
    forge: Option<ForgeResources>,
    governor: BifrostResourceGovernor,
    /// Process-wide encoded body owner shared by HTTP and tonic surfaces.
    transport: crate::gate::limits::BifrostTransportAdmission,
}

impl BifrostRoleResources {
    /// Returns the sole process resource-health lifecycle signal.
    #[must_use]
    pub fn health(&self) -> BifrostResourceHealth {
        self.governor.inner.health.clone()
    }
    /// Returns the shared byte-weighted encoded-body admission handle.
    #[must_use]
    pub fn transport_admission(&self) -> crate::gate::limits::BifrostTransportAdmission {
        self.transport.clone()
    }
    /// Returns the Scribe capability when the role is enabled.
    #[must_use]
    pub fn scribe(&self) -> Option<ScribeResources> {
        self.scribe.clone()
    }

    /// Returns the Oracle capability when the role is enabled.
    #[must_use]
    pub fn oracle(&self) -> Option<OracleResources> {
        self.oracle.clone()
    }

    /// Returns the Forge capability when the role is enabled.
    #[must_use]
    pub fn forge(&self) -> Option<ForgeResources> {
        self.forge.clone()
    }

    /// Returns the immutable checked plan shared by every issued capability.
    #[must_use]
    pub fn plan(&self) -> ResourcePlan {
        self.governor.plan()
    }

    /// Returns which observation source resolved each bound in this plan.
    ///
    /// Owner-inspection journeys assert that an injected test observation and a
    /// live detection reach the same composition through the same policy stage,
    /// which requires reading the resolved sources from the composition itself.
    #[must_use]
    pub fn sources(&self) -> ResolvedResourceSources {
        self.governor.sources()
    }

    /// Captures exact live ownership across every role sharing this root.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Poisoned`] when a trustworthy live
    /// snapshot is unavailable.
    pub fn snapshot(&self) -> Result<ResourceSnapshot, BifrostResourceError> {
        self.governor.snapshot()
    }
}

/// Protected Scribe memory capability backed by the shared process root.
#[derive(Debug, Clone)]
pub struct ScribeResources {
    governor: BifrostResourceGovernor,
    /// Registered Scribe stage and output roots, present on live boot.
    volumes: Option<BifrostVolumeRoots>,
    /// Existing-role bounded concurrency for request-local live-tail followers.
    follower_permits: Arc<Semaphore>,
}

impl ScribeResources {
    /// Returns the shared process resource-health lifecycle signal.
    #[must_use]
    pub fn health(&self) -> BifrostResourceHealth {
        self.governor.inner.health.clone()
    }
    /// Acquires one exact root-accounted quantum for a Scribe-role physical follower.
    ///
    /// The returned move-only owner holds both one existing Scribe concurrency
    /// permit and a charge against the shared Bifrost cap until the follower
    /// stream terminates. This adds no governor or root.
    ///
    /// # Errors
    ///
    /// Returns the existing root refusal when the requested positive quantum
    /// cannot be admitted without exceeding the shared cap.
    pub fn try_acquire_follower(
        &self,
        request_id: &RequestId,
        estimated_bytes: usize,
    ) -> Result<ScribeFollowerLease, BifrostResourceError> {
        if estimated_bytes == 0 {
            return Err(BifrostResourceError::InvalidPlan {
                detail: "Scribe follower memory must be positive".to_owned(),
            });
        }
        let permit = Arc::clone(&self.follower_permits)
            .try_acquire_owned()
            .map_err(|_| BifrostResourceError::Occupied {
                detail: "Scribe follower concurrency is saturated".to_owned(),
            })?;
        let lease = self.governor.try_acquire_scribe_memory(
            ScribeMemoryRequest {
                bytes: estimated_bytes,
                category: crate::scribe::memory::MemoryCategory::Decode,
                shard: None,
            },
            None,
        )?;
        let pool: Arc<dyn MemoryPool> = Arc::new(GreedyMemoryPool::new(estimated_bytes));
        Ok(ScribeFollowerLease {
            request_id: request_id.clone(),
            _permit: permit,
            lease,
            pool,
        })
    }

    /// Captures the Scribe-compatible projection of authoritative root state.
    pub(crate) fn memory_snapshot(&self) -> crate::scribe::memory::MemorySnapshot {
        let state = self
            .governor
            .inner
            .state
            .lock()
            .expect("Scribe root state lock");
        let plan = self.governor.plan();
        crate::scribe::memory::MemorySnapshot {
            pod_limit_bytes: plan.memory_limit_bytes,
            bifrost_limit_bytes: plan.managed_memory_bytes,
            bifrost_total_bytes: state.governed_total(),
            scribe_total_bytes: state.held(MemoryHolder::Scribe),
            scribe_limit_bytes: self.limit_bytes(),
            oracle_total_bytes: state.held(MemoryHolder::Oracle),
            oracle_limit_bytes: plan.managed_memory_bytes,
            categories: state.scribe_category_bytes,
            cgroup_current_bytes: self.governor.cgroup_pressure().map(|(current, _)| current),
            cgroup_limit_bytes: self.governor.inner.cgroup_limit_bytes,
            ingress_occupancy_bytes: state.held(MemoryHolder::Scribe),
            ingress_limit_bytes: self.ingress_limit_bytes(),
            ingress_high_water_bytes: 0,
            ingress_low_water_bytes: 0,
        }
    }

    /// Returns root-attributed bytes for each of `shard_count` shards.
    ///
    /// # Panics
    ///
    /// Panics when the root state lock is poisoned.
    #[must_use]
    pub(crate) fn shard_snapshot(&self, shard_count: usize) -> Vec<usize> {
        let state = self
            .governor
            .inner
            .state
            .lock()
            .expect("Scribe root state lock");
        (0..shard_count)
            .map(|shard| {
                state
                    .scribe_shard_bytes
                    .get(&shard)
                    .copied()
                    .unwrap_or_default()
            })
            .collect()
    }

    /// Emits only the root-owned closed resource gauge family.
    pub(crate) fn emit_root_resource_gauges(&self) {
        if let Err(error) = self.governor.emit_metrics() {
            tracing::error!(%error, "Scribe root resource gauges unavailable");
        }
    }

    /// Poisons the sole root after an ownership invariant failure.
    pub(crate) fn poison(&self) {
        self.governor.poison("Scribe ownership invariant failed");
    }

    /// Reports whether the sole root has entered fail-stop health.
    #[must_use]
    pub(crate) fn is_poisoned(&self) -> bool {
        self.governor.inner.health.reason().is_some()
    }

    /// Captures reconciled attribution for focused owner rollback tests.
    #[cfg(test)]
    pub(crate) fn accounting_snapshot_for_test(&self) -> ResourceAttributionSnapshot {
        let state = self
            .governor
            .inner
            .state
            .lock()
            .expect("test root state lock");
        ResourceAttributionSnapshot {
            category_bytes: state.scribe_category_bytes,
            shard_bytes: state.scribe_shard_bytes.clone(),
        }
    }

    /// Exercises ordinary ingress refusal after test-owned poison.
    #[cfg(test)]
    pub(crate) fn try_reserve(
        &self,
        category: ScribeMemoryCategory,
        bytes: usize,
    ) -> Result<ScribeMemoryLease, crate::contracts::ScribeError> {
        self.try_reserve_ingress(category, bytes)
    }

    /// Reports live cgroup pressure through the root's throttled sampler.
    #[must_use]
    pub(crate) fn cgroup_tripwire_engaged(&self) -> bool {
        matches!(self.governor.cgroup_pressure(), Some((current, limit)) if current.saturating_mul(100) >= limit.saturating_mul(90))
    }
    /// Returns the registered durable Scribe staging root.
    #[must_use]
    pub fn stage_root(&self) -> Option<&Path> {
        self.volumes
            .as_ref()
            .map(|volumes| volumes.scribe_stage.as_path())
    }

    /// Returns the Scribe output scratch namespace reconciled at registration.
    #[must_use]
    pub fn output_scratch(&self) -> Option<ScratchVolume> {
        self.volumes.as_ref().map(|volumes| ScratchVolume {
            root: volumes.scribe_output_scratch.clone(),
        })
    }
    /// Acquires one exact root-owned Scribe lease with lifecycle attribution.
    ///
    /// # Errors
    ///
    /// Returns a typed root refusal without changing capacity or attribution.
    pub(crate) fn try_acquire_memory(
        &self,
        request: ScribeMemoryRequest,
    ) -> Result<ScribeMemoryLease, BifrostResourceError> {
        self.governor.try_acquire_scribe_memory(request, None)
    }

    /// Returns the shared Bifrost cap, the most Scribe can ever hold.
    #[must_use]
    pub(crate) fn limit_bytes(&self) -> usize {
        self.governor.plan().managed_memory_bytes
    }

    /// Returns the full checked Scribe ceiling for ingress ownership.
    ///
    /// This maximum envelope is distinct from per-bucket rotation targets and
    /// bounds a whole accepted request under the shared process-root governor.
    /// It is the shared cap: Scribe holds no private partition.
    #[must_use]
    pub(crate) fn ingress_limit_bytes(&self) -> usize {
        self.limit_bytes()
    }

    /// Returns the maximum intrinsic Scribe envelope available at server boot.
    ///
    /// This public projection exposes capacity only, never mutable accounting,
    /// so the server can reject an unreplayable configured request shape before
    /// it starts a Scribe role. Runtime admission remains owned by this capability.
    #[must_use]
    pub fn maximum_ingress_envelope_bytes(&self) -> usize {
        self.ingress_limit_bytes()
    }

    /// Acquires ingress ownership directly from the root capability.
    ///
    /// # Errors
    ///
    /// Returns the stable Scribe busy/internal projection when root admission
    /// refuses or cannot account the request.
    pub(crate) fn try_reserve_ingress(
        &self,
        category: ScribeMemoryCategory,
        bytes: usize,
    ) -> Result<ScribeMemoryLease, crate::contracts::ScribeError> {
        self.governor
            .try_acquire_scribe_memory(
                ScribeMemoryRequest {
                    bytes,
                    category,
                    shard: None,
                },
                Some(self.ingress_limit_bytes()),
            )
            .map_err(scribe_resource_error)
    }

    /// Reports whether one rejected request exceeded Scribe's ingress sublimit.
    ///
    /// The ingress owner uses this after a failed root acquisition to preserve
    /// the closed rejection metric vocabulary without exposing root-ledger
    /// internals or creating a second capacity owner.
    #[must_use]
    pub(crate) fn ingress_sublimit_exceeded(&self, bytes: usize) -> bool {
        self.governor.snapshot().map_or(true, |snapshot| {
            snapshot
                .scribe_memory_used_bytes
                .checked_add(bytes)
                .is_none_or(|next| next > self.ingress_limit_bytes())
        })
    }

    /// Acquires maintenance ownership directly from the root capability.
    ///
    /// # Errors
    ///
    /// Returns the stable Scribe busy/internal projection when root admission
    /// refuses or cannot account the request.
    pub(crate) fn try_reserve_maintenance(
        &self,
        category: ScribeMemoryCategory,
        bytes: usize,
    ) -> Result<ScribeMemoryLease, crate::contracts::ScribeError> {
        self.try_acquire_memory(ScribeMemoryRequest {
            bytes,
            category,
            shard: None,
        })
        .map_err(scribe_resource_error)
    }

    /// Captures the current root capacity epoch before an admission attempt.
    #[must_use]
    pub fn memory_epoch(&self) -> u64 {
        self.governor.memory_epoch()
    }

    /// Waits until release, resize, or poison advances the root capacity epoch.
    ///
    /// Waiting never grants bytes. The caller must retry root admission after
    /// every successful wake.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Poisoned`] when root accounting becomes
    /// untrustworthy before or during the wait.
    pub async fn wait_for_memory_change(
        &self,
        observed_epoch: u64,
    ) -> Result<u64, BifrostResourceError> {
        self.governor.wait_for_memory_change(observed_epoch).await
    }

    /// Captures the authoritative root Scribe projection.
    pub fn snapshot(&self) -> Result<ResourceSnapshot, BifrostResourceError> {
        self.governor.snapshot()
    }

    /// Reports whether `other` was issued from this exact process root.
    ///
    /// This is lifecycle observation for tests and inspection; it grants no
    /// ability to construct a root.
    #[must_use]
    pub fn shares_root_with(&self, other: &BifrostRoleResources) -> bool {
        Arc::ptr_eq(&self.governor.inner, &other.governor.inner)
    }
}

/// Sole issuer of Oracle query leases against the shared process root.
#[derive(Debug, Clone)]
pub struct OracleResources {
    governor: BifrostResourceGovernor,
    /// The one governed `DataFusion` root every query on this pod shares.
    ///
    /// Cloning this capability shares the same root, which is the point: two
    /// clones must not be able to hand out two independent memory envelopes.
    memory_root: Arc<GovernedMemoryRoot>,
    /// Test-tier controller that makes one real query hold governed memory.
    ///
    /// It lives beside the root rather than inside it because it is not part of
    /// the governed accounting: it only borrows the next query's own view, so
    /// the bytes it holds are charged exactly as that query's own consumers
    /// would be.
    #[cfg(feature = "test-support")]
    memory_hold: Arc<OracleQueryMemoryHold>,
}

/// Test-tier controller that parks governed memory inside one real query view.
///
/// Journeys need a pod whose shared Oracle root is genuinely occupied while a
/// second query arrives. Faking that with a counter would prove nothing about
/// the root, so this arms a byte count, and the next admitted query grows a
/// named reservation for it through the query view `DataFusion` itself would
/// use. Nothing here bypasses the governor.
#[cfg(feature = "test-support")]
#[derive(Debug, Default)]
pub struct OracleQueryMemoryHold {
    /// Bytes the next admitted query must reserve, taken when it engages.
    armed: Mutex<Option<usize>>,
    /// The live reservation, retained until the journey releases it.
    held: Mutex<Option<MemoryReservation>>,
    /// Wakes a waiter once the reservation is live.
    reached: Notify,
    /// Set with `reached` so a waiter arriving afterwards still observes it.
    engaged: std::sync::atomic::AtomicBool,
    /// Parks the next admitted queries claim, in admission order.
    parks: Mutex<std::collections::VecDeque<Arc<OracleQueryPark>>>,
}

/// Test-tier park that holds one admitted query after its first batch frame.
///
/// Journeys need admitted queries that are provably running, having already
/// streamed rows, while a sibling fails and a third query waits. The query's
/// own frame loop parks here after emitting its first batch and, once the
/// journey resolves the park, either resumes or fails through a real refused
/// growth of its own admitted `DataFusion` pool. Nothing here bypasses the
/// governor or synthesizes the failure's error.
#[cfg(feature = "test-support")]
#[derive(Debug, Default)]
pub struct OracleQueryPark {
    /// Set once the query has emitted its first batch and parked.
    entered: std::sync::atomic::AtomicBool,
    /// Wakes a waiter once the query parks.
    entered_signal: Notify,
    /// The journey's resolution: `Some(true)` exhausts, `Some(false)` resumes.
    resolution: Mutex<Option<bool>>,
    /// Wakes the parked query once a resolution is set.
    resolved: Notify,
}

#[cfg(feature = "test-support")]
impl OracleQueryPark {
    /// Waits until the claiming query has emitted rows and parked.
    pub async fn wait_entered(&self) {
        loop {
            let entered = self.entered_signal.notified();
            if self.entered.load(std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            entered.await;
        }
    }

    /// Resumes the parked query so it drains to its ordinary terminal.
    pub fn resume(&self) {
        self.resolve(false);
    }

    /// Fails the parked query's next allocation against its own admitted pool.
    pub fn exhaust(&self) {
        self.resolve(true);
    }

    /// Records one resolution and wakes the parked query.
    fn resolve(&self, exhaust: bool) {
        if let Ok(mut resolution) = self.resolution.lock() {
            *resolution = Some(exhaust);
        }
        self.resolved.notify_waiters();
    }

    /// Parks the calling query until resolved and returns whether to exhaust.
    ///
    /// Called once by the query's frame loop after its first batch frame.
    /// A poisoned resolution lock resumes the query rather than parking it
    /// forever.
    pub(crate) async fn park(&self) -> bool {
        self.entered
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.entered_signal.notify_waiters();
        loop {
            let resolved = self.resolved.notified();
            match self.resolution.lock().map(|resolution| *resolution) {
                Ok(Some(exhaust)) => return exhaust,
                Ok(None) => {}
                Err(_) => return false,
            }
            resolved.await;
        }
    }
}

#[cfg(feature = "test-support")]
impl OracleQueryMemoryHold {
    /// Arms the next admitted query to hold exactly `bytes` of governed memory.
    pub fn arm(&self, bytes: usize) {
        self.engaged
            .store(false, std::sync::atomic::Ordering::SeqCst);
        if let Ok(mut armed) = self.armed.lock() {
            *armed = Some(bytes);
        }
    }

    /// Grows and retains the armed reservation through one query's own view.
    ///
    /// Called once from admission with the view that query will execute
    /// against. A refusal leaves the hold disarmed and unsignalled, so a
    /// journey that armed more than the root can cover fails at its wait rather
    /// than silently proceeding against an unoccupied root.
    fn engage(&self, pool: &Arc<dyn MemoryPool>) {
        let Ok(mut armed) = self.armed.lock() else {
            return;
        };
        let Some(bytes) = armed.take() else {
            return;
        };
        drop(armed);
        let reservation = MemoryConsumer::new("oracle-test-memory-hold").register(pool);
        if reservation.try_grow(bytes).is_err() {
            return;
        }
        if let Ok(mut held) = self.held.lock() {
            *held = Some(reservation);
        }
        self.engaged
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.reached.notify_waiters();
    }

    /// Arms the next admitted query without a park to park after its first batch.
    ///
    /// Parks are claimed in admission order, so arming two parks the next two
    /// admitted queries in the order they are admitted.
    pub fn park_next_after_rows(&self) -> Arc<OracleQueryPark> {
        let park = Arc::new(OracleQueryPark::default());
        if let Ok(mut parks) = self.parks.lock() {
            parks.push_back(Arc::clone(&park));
        }
        park
    }

    /// Claims the oldest armed park for the query being admitted, if any.
    fn claim_park(&self) -> Option<Arc<OracleQueryPark>> {
        self.parks.lock().ok()?.pop_front()
    }

    /// Waits until an admitted query is holding the armed reservation.
    pub async fn wait_engaged(&self) {
        while !self.engaged.load(std::sync::atomic::Ordering::SeqCst) {
            self.reached.notified().await;
        }
    }

    /// Drops the retained reservation, returning its bytes to the shared root.
    pub fn release(&self) {
        if let Ok(mut held) = self.held.lock() {
            held.take();
        }
        self.engaged
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

impl OracleResources {
    /// Returns the shared process resource-health lifecycle signal.
    #[must_use]
    pub fn health(&self) -> BifrostResourceHealth {
        self.governor.inner.health.clone()
    }
    /// Splits one named child from an already-admitted Oracle query pool.
    ///
    /// # Errors
    ///
    /// Returns `DataFusion` resource exhaustion when the query-local pool
    /// cannot cover `bytes`; this capability never admits root capacity again.
    pub(crate) fn try_split_query_memory(
        &self,
        pool: &Arc<dyn MemoryPool>,
        consumer: &'static str,
        bytes: usize,
    ) -> Result<OracleQueryMemoryReservation, DataFusionError> {
        OracleQueryMemoryReservation::try_new(pool, self.governor.clone(), consumer, bytes)
    }

    /// Acquires one remote-worker slot quantum alongside other Oracle owners.
    ///
    /// A follower holds concurrency, not resident memory: its bytes are charged
    /// only as its `DataFusion` consumers grow through the shared Oracle root,
    /// exactly as a leader's are. Query, metadata, and sibling worker owners
    /// remain independently attributable in that same root ledger.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Occupied`] when the aggregate slot ledger
    /// cannot cover this class's units without spending the Interactive floor,
    /// or a poison/invalid-plan error when root accounting or role configuration
    /// is not trustworthy. Refusal changes no counters.
    pub fn try_acquire_worker(
        &self,
        class: OracleWorkerClass,
    ) -> Result<OracleWorkerResources, BifrostResourceError> {
        let plan = self.governor.plan();
        let query_class = class.query_class();
        let slot_units = class.slot_units();
        // Charge the one aggregate slot ledger before any memory moves. A
        // follower fragment competes for the same local units a queued leader
        // is waiting on, so admitting it without charging here is exactly how
        // the Interactive floor would be spent by remote Analytical work.
        let next_slots = {
            let mut state = self.governor.lock_state()?;
            let (next_slots, next_analytical) =
                self.governor
                    .charge_oracle_slots(&state, query_class, slot_units)?;
            state.oracle_query_slot_units = next_slots;
            state.oracle_analytical_slot_units = next_analytical;
            next_slots
        };
        let slots = OracleSlotCharge {
            query_class,
            slot_units,
            governor: self.governor.clone(),
            released: false,
        };
        let granted_memory_bytes =
            BifrostResourceGovernor::oracle_memory_grant(plan, slot_units, next_slots);
        // A follower is a query too: it takes a private view over the same
        // shared root the leader queries use, never an independent pool whose
        // ceiling could sum above what the pod owns.
        let memory_pool = self
            .memory_root
            .query_view(granted_memory_bytes, &Arc::new(AtomicUsize::new(0)));
        // Locality is zero here: a remote worker reads the files the leader
        // dispatched to it, so its partition ceiling comes from the grant it
        // was admitted with rather than from any caller-supplied hint.
        let admitted_target_partitions =
            oracle_target_partitions(plan.effective_cpu, 0.0, granted_memory_bytes)?;
        Ok(OracleWorkerResources {
            _slots: slots,
            memory_pool,
            granted_memory_bytes,
            admitted_target_partitions,
        })
    }

    /// Returns the aggregate bytes every live Oracle query holds at the root.
    ///
    /// This is the shared figure, not one query's: it is what proves two
    /// concurrent queries compete beneath one envelope rather than beside it.
    #[must_use]
    pub fn shared_memory_reserved(&self) -> usize {
        self.memory_root.reserved()
    }

    /// Returns the cooperative maximum the shared Oracle root arbitrates.
    #[must_use]
    pub fn shared_memory_limit(&self) -> usize {
        self.memory_root.limit_bytes()
    }

    /// Captures the Oracle slot-and-scratch epoch before an admission attempt.
    #[must_use]
    pub fn capacity_epoch(&self) -> u64 {
        self.governor.oracle_capacity_epoch()
    }

    /// Waits until a slot or scratch return, or poison, advances the Oracle capacity epoch.
    ///
    /// Waiting never grants capacity. A queued Oracle leader uses this to learn
    /// that a follower or sibling query returned slot units or scratch, then
    /// re-runs its own scheduler; it must never treat a wake as an admission.
    /// Query-memory growth and shrink do not advance this epoch because Oracle
    /// admission never refuses on resident query memory.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Poisoned`] when root accounting becomes
    /// untrustworthy before or during the wait.
    pub async fn wait_for_capacity_change(
        &self,
        observed_epoch: u64,
    ) -> Result<u64, BifrostResourceError> {
        self.governor
            .wait_for_oracle_capacity_change(observed_epoch)
            .await
    }

    /// Returns aggregate slot units currently held across leaders and followers.
    ///
    /// Reports zero when the shared ledger is poisoned, which is the same
    /// degradation admission already applies: a poisoned ledger refuses every
    /// acquisition, so an optimistic reading cannot admit work.
    #[must_use]
    pub fn live_slot_units(&self) -> u64 {
        self.governor
            .lock_state()
            .map_or(0, |state| u64::from(state.oracle_query_slot_units))
    }

    /// Returns the immutable pod-local class split this capability admits under.
    #[must_use]
    pub fn class_split(&self) -> OracleClassSplit {
        self.governor.oracle_class_split()
    }

    /// Publishes the boot-derived class split for every capability on this root.
    ///
    /// Boot calls this once after local configuration and any approved
    /// calibration profile have resolved. A second call is a no-op and returns
    /// the already published split, so composition can never install two
    /// competing local capacities.
    pub fn install_class_split(&self, split: OracleClassSplit) -> OracleClassSplit {
        self.governor.install_oracle_class_split(split)
    }

    /// Returns the fixed-purpose Oracle metadata admission capability.
    #[must_use]
    pub fn metadata(&self) -> OracleMetadataResources {
        OracleMetadataResources {
            governor: self.governor.clone(),
        }
    }

    /// Atomically acquires one exact query envelope alongside compatible owners.
    ///
    /// Slot units are additive and analytical admission preserves one
    /// interactive quantum. The returned query owns a bounded `DataFusion`
    /// pool and a spill limit; it charges no disk.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Occupied`] when slot units or the
    /// protected interactive reserve cannot cover the exact request. Returns
    /// [`BifrostResourceError::InvalidPlan`] for zero, overflowing, inactive,
    /// or invalid partition demand, and a poison error for divergent root
    /// accounting. No root counter changes on refusal.
    pub fn try_acquire_query(
        &self,
        request: OracleResourceRequest,
    ) -> Result<OracleQueryResources, BifrostResourceError> {
        let resources = self
            .governor
            .try_acquire_oracle(request, &self.memory_root)?;
        #[cfg(feature = "test-support")]
        let resources = {
            let mut resources = resources;
            self.memory_hold.engage(&resources.memory_pool);
            resources.park = self.memory_hold.claim_park();
            resources
        };
        Ok(resources)
    }

    /// Returns the test-tier controller that parks memory in one query view.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn memory_hold(&self) -> Arc<OracleQueryMemoryHold> {
        Arc::clone(&self.memory_hold)
    }

    /// Captures exact live ownership for inspection and cleanup assertions.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Poisoned`] when a trustworthy live
    /// snapshot is unavailable.
    pub fn snapshot(&self) -> Result<ResourceSnapshot, BifrostResourceError> {
        self.governor.snapshot()
    }

    /// Reports whether `other` was issued from this exact process root.
    #[must_use]
    pub fn shares_root_with(&self, other: &BifrostRoleResources) -> bool {
        Arc::ptr_eq(&self.governor.inner, &other.governor.inner)
    }
}

/// Forge rewrite memory capability backed by the shared process root.
///
/// Every managed rewrite attempt draws its `DataFusion` pool from here, so
/// Forge competes for the same shared cap Scribe, Oracle, and transport hold
/// instead of reserving an estimated share in advance.
#[derive(Debug, Clone)]
pub struct ForgeResources {
    /// The one governed `DataFusion` root every Forge rewrite shares.
    memory_root: Arc<GovernedMemoryRoot>,
}

impl ForgeResources {
    /// Issues one rewrite attempt's pool over the shared Forge root.
    ///
    /// The view's ceiling is the shared cap itself: Forge has no independent
    /// finite budget. Fallible growth refuses when the shared cap is full, so
    /// spillable operators spill and non-spillable ones fail only this
    /// attempt. Infallible growth above the cap is accounted as headroom and
    /// returned when the consumer shrinks or drops.
    #[must_use]
    pub fn rewrite_memory_pool(&self) -> Arc<dyn MemoryPool> {
        self.memory_root.query_view(
            self.memory_root.limit_bytes(),
            &Arc::new(AtomicUsize::new(0)),
        )
    }

    /// Captures exact live ownership across every role sharing this root.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Poisoned`] when a trustworthy live
    /// snapshot is unavailable.
    pub fn snapshot(&self) -> Result<ResourceSnapshot, BifrostResourceError> {
        self.memory_root.governor.snapshot()
    }

    /// Fills every free byte of the shared root through one Forge view.
    ///
    /// Test-tier pressure for journeys proving a refused Forge growth fails
    /// only its attempt: the reservation is grown fallibly in halving chunks
    /// until one byte more is refused, so the root is exactly full and every
    /// later fallible growth on any role sharing it is refused. Dropping the
    /// returned reservation returns the bytes.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn occupy_root_for_test(&self) -> MemoryReservation {
        let reservation =
            MemoryConsumer::new("forge-test-root-occupant").register(&self.rewrite_memory_pool());
        let mut chunk = self.memory_root.limit_bytes();
        while chunk > 0 {
            if reservation.try_grow(chunk).is_err() {
                chunk /= 2;
            }
        }
        reservation
    }
}

#[derive(Debug)]
struct ResourceGovernorInner {
    plan: ResourcePlan,
    /// Immutable pod-local Oracle class split, installed once during boot.
    ///
    /// Boot installs the split derived from configuration or an approved
    /// calibration profile; every capability cloned from this root therefore
    /// reads one value. Callers that never install fall back to the plan's own
    /// derivation so focused tests and inspection paths stay consistent.
    oracle_class_split: OnceLock<OracleClassSplit>,
    sources: ResolvedResourceSources,
    state: Mutex<ResourceState>,
    /// Lost-wakeup-safe notification paired with `ResourceState::memory_epoch`.
    memory_changed: Notify,
    /// Lost-wakeup-safe notification paired with `ResourceState::oracle_capacity_epoch`.
    oracle_capacity_changed: Notify,
    /// Cgroup hard limit used by the live external-pressure tripwire.
    cgroup_limit_bytes: Option<usize>,
    /// Live cgroup usage cached for at most one second under the root owner.
    cgroup_current: Mutex<Option<(Instant, Option<usize>)>>,
    /// Lock-free first-poison signal observed by application supervision.
    health: BifrostResourceHealth,
}

/// Resolves the server headroom and the one shared Bifrost cap.
///
/// The cap is `process limit - server minimum`, optionally lowered by an
/// operator. The server minimum may only rise above
/// [`DEFAULT_SERVER_MEMORY_MIN_BYTES`] and the operator cap may only fall, so
/// neither setting can raise the cap above what the process limit leaves.
///
/// # Errors
///
/// Returns [`BifrostResourceError::InvalidPlan`] when the server minimum is
/// below the default, when it leaves no Bifrost memory, or when the operator
/// cap is zero or exceeds `process limit - server minimum`. Nothing clamps: a
/// setting the pod cannot honour refuses boot.
fn shared_memory_cap(
    memory_limit_bytes: usize,
    server_memory_min_bytes: Option<usize>,
    bifrost_memory_limit_bytes: Option<usize>,
) -> Result<(usize, usize), BifrostResourceError> {
    let server_min = server_memory_min_bytes.unwrap_or(DEFAULT_SERVER_MEMORY_MIN_BYTES);
    if server_min < DEFAULT_SERVER_MEMORY_MIN_BYTES {
        return Err(BifrostResourceError::InvalidPlan {
            detail: format!(
                "server memory minimum {server_min} is below {DEFAULT_SERVER_MEMORY_MIN_BYTES}"
            ),
        });
    }
    let available = memory_limit_bytes
        .checked_sub(server_min)
        .filter(|available| *available > 0)
        .ok_or_else(|| BifrostResourceError::InvalidPlan {
            detail: format!(
                "process memory {memory_limit_bytes} leaves no Bifrost memory above the \
                 {server_min}-byte server minimum"
            ),
        })?;
    let cap = match bifrost_memory_limit_bytes {
        None => available,
        Some(cap) if cap > 0 && cap <= available => cap,
        Some(cap) => {
            return Err(BifrostResourceError::InvalidPlan {
                detail: format!(
                    "Bifrost memory limit {cap} must be positive and at most the {available} \
                     bytes process memory {memory_limit_bytes} leaves above the \
                     {server_min}-byte server minimum"
                ),
            });
        }
    };
    Ok((server_min, cap))
}

/// Resolves the disposable scratch budget and the availability it was cut from.
///
/// The configured limit never raises detected capacity, and the filesystem
/// floor is reserved before anything is offered, so a full disk yields zero
/// rather than a negative figure. Oracle cannot plan against zero disposable
/// scratch, so that combination refuses the plan here instead of failing at
/// the first spill. The second returned figure is the post-floor availability
/// the caller records as the plan's scratch source evidence.
///
/// # Errors
///
/// Returns [`BifrostResourceError::InvalidPlan`] when availability cannot
/// preserve the filesystem floor, or when Oracle is enabled and no disposable
/// scratch remains after it.
fn scratch_budget(
    snapshot: &SystemResourceSnapshot,
    policy: &BifrostResourcePolicy,
) -> Result<(u64, u64), BifrostResourceError> {
    let configured_scratch = policy
        .scratch_limit_bytes
        .map_or(snapshot.scratch_capacity_bytes, |limit| {
            limit.min(snapshot.scratch_capacity_bytes)
        });
    let available_after_floor = snapshot
        .scratch_available_bytes
        .checked_sub(MIN_SCRATCH_FREE_BYTES)
        .ok_or_else(|| BifrostResourceError::InvalidPlan {
            detail: format!(
                "scratch availability {} cannot preserve filesystem floor {MIN_SCRATCH_FREE_BYTES}",
                snapshot.scratch_available_bytes
            ),
        })?;
    let scratch_limit_bytes = configured_scratch.min(available_after_floor);
    if policy.roles.contains(&BifrostRole::Oracle) && scratch_limit_bytes == 0 {
        return Err(BifrostResourceError::InvalidPlan {
            detail: "Oracle requires positive disposable scratch after the filesystem floor"
                .to_owned(),
        });
    }
    Ok((scratch_limit_bytes, available_after_floor))
}

impl BifrostResourceGovernor {
    /// Constructs the governor from deterministic inputs and checked policy.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::InvalidPlan`] when an override inflates a
    /// detected resource, weakens a minimum floor, or checked plan arithmetic
    /// cannot fit every enabled role.
    pub(crate) fn from_snapshot(
        snapshot: SystemResourceSnapshot,
        policy: BifrostResourcePolicy,
    ) -> Result<Self, BifrostResourceError> {
        if snapshot.memory_limit_bytes == 0 {
            return Err(BifrostResourceError::Unavailable {
                detail: "no positive memory bound was detected".to_owned(),
            });
        }
        let memory_limit_bytes = snapshot.memory_limit_bytes;
        let effective_cpu = cap_positive(snapshot.effective_cpu, policy.effective_cpu, "CPU")?;
        let oracle_query_slot_limit = policy.oracle_query_slot_limit;
        let (server_memory_min_bytes, managed_memory_bytes) = shared_memory_cap(
            memory_limit_bytes,
            policy.server_memory_min_bytes,
            policy.bifrost_memory_limit_bytes,
        )?;
        let (scratch_limit_bytes, available_after_floor) = scratch_budget(&snapshot, &policy)?;
        drop(policy.scratch_root);
        let root = Self {
            inner: Arc::new(ResourceGovernorInner {
                plan: ResourcePlan {
                    memory_limit_bytes,
                    effective_cpu,
                    oracle_query_slot_limit,
                    server_memory_min_bytes,
                    managed_memory_bytes,
                    scribe_enabled: policy.roles.contains(&BifrostRole::Scribe),
                    oracle_enabled: policy.roles.contains(&BifrostRole::Oracle),
                    forge_enabled: policy.roles.contains(&BifrostRole::Forge),
                    scratch_limit_bytes,
                },
                sources: ResolvedResourceSources {
                    memory: snapshot.memory_source,
                    cpu: if policy
                        .effective_cpu
                        .is_some_and(|limit| limit <= snapshot.effective_cpu)
                    {
                        ResourceSource::Override
                    } else {
                        snapshot.cpu_source
                    },
                    scratch: if policy.scratch_limit_bytes.is_some_and(|limit| {
                        limit <= snapshot.scratch_capacity_bytes && limit <= available_after_floor
                    }) {
                        ResourceSource::Override
                    } else {
                        ResourceSource::Filesystem
                    },
                },
                oracle_class_split: OnceLock::new(),
                state: Mutex::new(ResourceState::default()),
                memory_changed: Notify::new(),
                oracle_capacity_changed: Notify::new(),
                cgroup_limit_bytes: crate::scribe::memory::read_cgroup_limit(),
                cgroup_current: Mutex::new(None),
                health: BifrostResourceHealth::default(),
            }),
        };
        root.record_plan_metrics();
        Ok(root)
    }

    /// Publishes immutable server-headroom, shared-cap, and scratch gauges.
    fn record_plan_metrics(&self) {
        let plan = self.plan();
        let planned = [
            ("server", "memory", plan.server_memory_min_bytes),
            ("bifrost", "memory", plan.managed_memory_bytes),
        ];
        for (role, resource, bytes) in planned {
            metrics::gauge!(
                "bifrost_resource_planned_bytes",
                "role" => role,
                "resource" => resource
            )
            .set(bytes.to_f64().unwrap_or(f64::MAX));
        }
        metrics::gauge!(
            "bifrost_resource_planned_bytes",
            "role" => "shared",
            "resource" => "scratch"
        )
        .set(plan.scratch_limit_bytes.to_f64().unwrap_or(f64::MAX));
    }

    /// Returns the immutable boot resource calculation.
    #[must_use]
    pub(crate) fn plan(&self) -> ResourcePlan {
        self.inner.plan
    }

    /// Returns the portable source selected for each resource dimension.
    #[must_use]
    pub(crate) fn sources(&self) -> ResolvedResourceSources {
        self.inner.sources
    }

    /// Returns throttled live cgroup usage and its root-owned hard limit.
    fn cgroup_pressure(&self) -> Option<(usize, usize)> {
        let current = if let Ok(cache) = self.inner.cgroup_current.lock()
            && let Some((sampled_at, value)) = *cache
            && sampled_at.elapsed() < Duration::from_secs(1)
        {
            value
        } else {
            let value = crate::scribe::memory::read_cgroup_current();
            if let Ok(mut cache) = self.inner.cgroup_current.lock() {
                *cache = Some((Instant::now(), value));
            }
            value
        }?;
        Some((current, self.inner.cgroup_limit_bytes?))
    }

    /// Captures exact live shared-cap ownership and its attribution.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Poisoned`] when the shared lock or prior
    /// accounting failure makes a trustworthy snapshot unavailable.
    pub(crate) fn snapshot(&self) -> Result<ResourceSnapshot, BifrostResourceError> {
        let state = self.lock_state()?;
        self.validate_scribe_reconciliation(&state)?;
        Ok(ResourceSnapshot {
            plan: self.plan(),
            governed_memory_used_bytes: state.governed_total(),
            scribe_memory_used_bytes: state.held(MemoryHolder::Scribe),
            oracle_memory_used_bytes: state.held(MemoryHolder::Oracle),
            forge_memory_used_bytes: state.held(MemoryHolder::Forge),
            transport_memory_used_bytes: state.held(MemoryHolder::Transport),
            oracle_active_queries: state.oracle_active_queries,
            oracle_interactive_queries: state.oracle_interactive_queries,
            oracle_analytical_queries: state.oracle_analytical_queries,
            oracle_query_slot_units: state.oracle_query_slot_units,
            oracle_query_memory_used_bytes: state.oracle_query_memory_used_bytes,
            infallible_headroom_bytes: state.infallible_headroom_bytes,
            oracle_query_active: state.oracle_active_queries > 0,
        })
    }

    /// Captures closed Scribe attribution after validating it against role use.
    ///
    /// # Errors
    ///
    /// Returns a poison error when category, shard, or generation attribution
    /// does not reconcile exactly to live Scribe ownership.
    #[cfg(test)]
    pub(crate) fn attribution_snapshot(
        &self,
    ) -> Result<ResourceAttributionSnapshot, BifrostResourceError> {
        let state = self.lock_state()?;
        self.validate_scribe_reconciliation(&state)?;
        Ok(ResourceAttributionSnapshot {
            category_bytes: state.scribe_category_bytes,
            shard_bytes: state.scribe_shard_bytes.clone(),
        })
    }

    /// Validates every Scribe attribution projection against root role use.
    ///
    /// # Errors
    ///
    /// Returns a poison error when checked totals overflow or any attribution
    /// projection diverges from the authoritative role counters.
    fn validate_scribe_reconciliation(
        &self,
        state: &ResourceState,
    ) -> Result<(), BifrostResourceError> {
        let category_total = state
            .scribe_category_bytes
            .iter()
            .try_fold(0usize, |total, bytes| {
                total.checked_add(*bytes).ok_or_else(accounting_overflow)
            })?;
        let shard_total = state
            .scribe_shard_bytes
            .values()
            .try_fold(0usize, |total, bytes| {
                total.checked_add(*bytes).ok_or_else(accounting_overflow)
            })?;
        let held_total = state.held_bytes.iter().try_fold(0usize, |total, bytes| {
            total.checked_add(*bytes).ok_or_else(accounting_overflow)
        })?;
        let plan = self.plan();
        let scribe = state.held(MemoryHolder::Scribe);
        if category_total != scribe
            || shard_total > scribe
            || held_total > plan.managed_memory_bytes
        {
            // Emit the operands: which of the identities broke is the whole
            // diagnosis, and it is unrecoverable from the error string.
            tracing::error!(
                category_total,
                shard_total,
                held_total,
                scribe_memory_used_bytes = scribe,
                managed_memory_bytes = plan.managed_memory_bytes,
                "Scribe root attribution does not reconcile to live ownership"
            );
            self.inner
                .health
                .poison(BifrostResourcePoisonReason::Accounting);
            self.inner.memory_changed.notify_waiters();
            self.notify_oracle_capacity();
            return Err(BifrostResourceError::Poisoned {
                detail: "Scribe root attribution does not reconcile to live ownership".to_owned(),
            });
        }
        Ok(())
    }

    /// Emits the single allocator's bounded plan and live-ownership gauges.
    ///
    /// Labels use closed resource/role domains and never include tenant,
    /// table, query, path, or deployment identity.
    ///
    /// # Errors
    ///
    /// Returns a poison error when a trustworthy live snapshot is unavailable.
    pub(crate) fn emit_metrics(&self) -> Result<(), BifrostResourceError> {
        let snapshot = self.snapshot()?;
        let as_f64 = |bytes: usize| bytes.to_f64().unwrap_or(f64::MAX);
        metrics::gauge!("bifrost_resource_memory_bytes", "kind" => "managed")
            .set(as_f64(snapshot.plan.managed_memory_bytes));
        metrics::gauge!("bifrost_resource_memory_bytes", "kind" => "governed_used")
            .set(as_f64(snapshot.governed_memory_used_bytes));
        metrics::gauge!("bifrost_resource_memory_bytes", "kind" => "scribe_used")
            .set(as_f64(snapshot.scribe_memory_used_bytes));
        metrics::gauge!("bifrost_resource_memory_bytes", "kind" => "oracle_used")
            .set(as_f64(snapshot.oracle_memory_used_bytes));
        metrics::gauge!("bifrost_resource_memory_bytes", "kind" => "forge_used")
            .set(as_f64(snapshot.forge_memory_used_bytes));
        metrics::gauge!("bifrost_resource_memory_bytes", "kind" => "transport_used")
            .set(as_f64(snapshot.transport_memory_used_bytes));
        metrics::gauge!("bifrost_resource_memory_bytes", "kind" => "infallible_headroom")
            .set(as_f64(snapshot.infallible_headroom_bytes));
        metrics::gauge!("bifrost_resource_scratch_bytes", "kind" => "total").set(
            snapshot
                .plan
                .scratch_limit_bytes
                .to_f64()
                .unwrap_or(f64::MAX),
        );
        Ok(())
    }

    /// Atomically admits one Scribe owner and its complete attribution tuple.
    ///
    /// # Errors
    ///
    /// Returns a typed root refusal with no mutation when capacity or checked
    /// attribution arithmetic cannot accept the request.
    fn try_acquire_scribe_memory(
        &self,
        request: ScribeMemoryRequest,
        ingress_limit_bytes: Option<usize>,
    ) -> Result<ScribeMemoryLease, BifrostResourceError> {
        let mut state = self.lock_state()?;
        let plan = self.plan();
        if !plan.scribe_enabled {
            return Err(BifrostResourceError::InvalidPlan {
                detail: "Scribe resources requested while the role is inactive".to_owned(),
            });
        }
        let next = state
            .held(MemoryHolder::Scribe)
            .checked_add(request.bytes)
            .ok_or_else(accounting_overflow)?;
        let category_index = request.category as usize;
        let next_category = state.scribe_category_bytes[category_index]
            .checked_add(request.bytes)
            .ok_or_else(accounting_overflow)?;
        let next_shard = request
            .shard
            .map(|shard| {
                state
                    .scribe_shard_bytes
                    .get(&shard)
                    .copied()
                    .unwrap_or_default()
                    .checked_add(request.bytes)
                    .ok_or_else(accounting_overflow)
            })
            .transpose()?;
        if ingress_limit_bytes.is_some_and(|limit| next > limit) {
            record_memory_transition("scribe", "refused", state.held(MemoryHolder::Scribe));
            return Err(BifrostResourceError::Occupied {
                detail: "Scribe request exceeds the ingress sublimit".to_owned(),
            });
        }
        self.charge_locked(&mut state, MemoryHolder::Scribe, request.bytes)?;
        state.scribe_category_bytes[category_index] = next_category;
        if let (Some(shard), Some(bytes)) = (request.shard, next_shard) {
            state.scribe_shard_bytes.insert(shard, bytes);
        }
        record_memory_transition("scribe", "acquired", next);
        Ok(ScribeMemoryLease {
            root: self.clone(),
            bytes: request.bytes,
            category: request.category,
            shard: request.shard,
            released: false,
        })
    }

    /// Returns the current capacity-change epoch from the authoritative lock.
    fn memory_epoch(&self) -> u64 {
        self.inner
            .state
            .lock()
            .map_or(u64::MAX, |state| state.memory_epoch)
    }

    /// Waits for a strictly newer epoch without granting capacity.
    ///
    /// # Errors
    ///
    /// Returns a poison error when the root becomes untrustworthy.
    async fn wait_for_memory_change(
        &self,
        observed_epoch: u64,
    ) -> Result<u64, BifrostResourceError> {
        loop {
            let notified = self.inner.memory_changed.notified();
            let current_epoch = { self.lock_state()?.memory_epoch };
            if current_epoch > observed_epoch {
                return Ok(current_epoch);
            }
            notified.await;
        }
    }

    /// Returns the Oracle slot-and-scratch epoch from the authoritative lock.
    fn oracle_capacity_epoch(&self) -> u64 {
        self.inner
            .state
            .lock()
            .map_or(u64::MAX, |state| state.oracle_capacity_epoch)
    }

    /// Waits for a strictly newer Oracle capacity epoch without granting capacity.
    ///
    /// # Errors
    ///
    /// Returns a poison error when the root becomes untrustworthy.
    async fn wait_for_oracle_capacity_change(
        &self,
        observed_epoch: u64,
    ) -> Result<u64, BifrostResourceError> {
        loop {
            let notified = self.inner.oracle_capacity_changed.notified();
            let current_epoch = { self.lock_state()?.oracle_capacity_epoch };
            if current_epoch > observed_epoch {
                return Ok(current_epoch);
            }
            notified.await;
        }
    }

    /// Advances the Oracle capacity epoch after slots or scratch returned.
    ///
    /// The caller holds the state lock and must call
    /// [`Self::notify_oracle_capacity`] after dropping it.
    fn advance_oracle_capacity(state: &mut ResourceState) {
        state.oracle_capacity_epoch = state.oracle_capacity_epoch.wrapping_add(1);
    }

    /// Wakes queued Oracle admission after a capacity return or poison.
    fn notify_oracle_capacity(&self) {
        self.inner.oracle_capacity_changed.notify_waiters();
    }

    /// Validates one query request against the active Oracle class contract.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::InvalidPlan`] when Oracle is inactive,
    /// any demand is zero, or the request differs from its exact class quantum.
    fn validate_oracle_request(
        plan: ResourcePlan,
        request: OracleResourceRequest,
    ) -> Result<(), BifrostResourceError> {
        if !plan.oracle_enabled {
            return Err(BifrostResourceError::InvalidPlan {
                detail: "Oracle resources requested while the role is inactive".to_owned(),
            });
        }
        if request.memory_bytes == 0 || request.slot_units == 0 {
            return Err(BifrostResourceError::InvalidPlan {
                detail: "Oracle query demands must be positive".to_owned(),
            });
        }
        let exact = OracleResourceRequest::for_class(request.query_class, request.local_ratio);
        if (request.memory_bytes, request.slot_units) != (exact.memory_bytes, exact.slot_units) {
            return Err(BifrostResourceError::InvalidPlan {
                detail: "Oracle query demand must match its class quantum".to_owned(),
            });
        }
        Ok(())
    }

    /// Returns the immutable pod-local Oracle class split for this root.
    ///
    /// Falls back to the plan's own derivation when boot never installed one,
    /// so focused tests and inspection paths observe the same total and
    /// Analytical maxima production admission enforces.
    fn oracle_class_split(&self) -> OracleClassSplit {
        *self.inner.oracle_class_split.get_or_init(|| {
            let raw = oracle_worker_slots(self.plan()).unwrap_or(1);
            OracleClassSplit::derive(u32::try_from(raw).unwrap_or(u32::MAX))
        })
    }

    /// Installs the boot-derived class split exactly once for this process root.
    ///
    /// Returns the installed split, which is the previously installed value
    /// when boot composition already published one.
    fn install_oracle_class_split(&self, split: OracleClassSplit) -> OracleClassSplit {
        *self.inner.oracle_class_split.get_or_init(|| split)
    }

    /// Charges `units` of the requested class against the aggregate slot ledger.
    ///
    /// Leaders admitted through `OracleAdmission` and followers admitted through
    /// [`OracleResources::try_acquire_worker`] share this one ledger, so the
    /// Interactive floor is preserved across both. The returned pair is the new
    /// aggregate and Analytical totals, already written to `state`.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Occupied`] when the aggregate would
    /// exceed the local total or Analytical would exceed its class maximum, and
    /// an overflow [`BifrostResourceError::InvalidPlan`] on checked-arithmetic
    /// failure. Nothing is written; the caller commits both totals once every
    /// other fallible step of its admission has succeeded.
    fn charge_oracle_slots(
        &self,
        state: &ResourceState,
        query_class: QueryClass,
        units: u32,
    ) -> Result<(u32, u32), BifrostResourceError> {
        let split = self.oracle_class_split();
        let next_total = state
            .oracle_query_slot_units
            .checked_add(units)
            .ok_or_else(accounting_overflow)?;
        let next_analytical = state
            .oracle_analytical_slot_units
            .checked_add(units * u32::from(query_class == QueryClass::Analytical))
            .ok_or_else(accounting_overflow)?;
        if next_total > split.total_units() || next_analytical > split.analytical_max_units {
            return Err(BifrostResourceError::Occupied {
                detail: "Oracle slot units exceed local class capacity".to_owned(),
            });
        }
        Ok((next_total, next_analytical))
    }

    /// Returns `units` of the requested class to the aggregate slot ledger.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Poisoned`] after poisoning the shared
    /// root when either counter would underflow.
    fn release_oracle_slots(
        &self,
        state: &mut ResourceState,
        query_class: QueryClass,
        units: u32,
    ) -> Result<(), BifrostResourceError> {
        let analytical = units * u32::from(query_class == QueryClass::Analytical);
        if state.oracle_query_slot_units < units || state.oracle_analytical_slot_units < analytical
        {
            return Err(self.poison_locked(state, "Oracle slot release underflow"));
        }
        state.oracle_query_slot_units -= units;
        state.oracle_analytical_slot_units -= analytical;
        Ok(())
    }

    /// Derives the memory ceiling one admitted query may grow into.
    ///
    /// The grant is `bifrost_cap * query_slots / sum(running_slots)`, clamped
    /// to `[ORACLE_PARTITION_WORKING_MEMORY_BYTES, ORACLE_PARTITION_MEMORY_BYTES]`.
    /// `running_slots` must already include the admitting query's own units, so
    /// an otherwise idle node grants the cap rather than dividing by zero.
    ///
    /// Two properties matter and neither is incidental.
    ///
    /// The numerator is the *configured* shared Bifrost cap, never currently
    /// free memory. Dividing free memory would make two identical queries receive
    /// different ceilings depending on what Scribe and Forge happened to be doing
    /// at that instant. Vertica, Redshift, and Doris all divide a fixed budget
    /// for exactly this reason; this is Doris's dynamic mode, which keeps the
    /// budget fixed and lets only the slot denominator move.
    ///
    /// Because every live query divides the same budget by the same live slot
    /// sum, the sum of outstanding grants stays inside the budget by
    /// construction. That is what lets admission be decided by slots alone: no
    /// overcommit ratio and no kill-on-OOM backstop is required to keep the node
    /// within its envelope.
    ///
    /// The grant is computed once here and held for the query's life. It is never
    /// recomputed as concurrency changes: shrinking a pool underneath an already
    /// running operator is how a non-spillable consumer hard-fails rather than
    /// spilling.
    fn oracle_memory_grant(
        plan: ResourcePlan,
        query_slot_units: u32,
        running_slot_units: u32,
    ) -> usize {
        let budget = plan.managed_memory_bytes;
        let denominator = running_slot_units.max(query_slot_units).max(1) as usize;
        let numerator = budget.saturating_mul(query_slot_units.max(1) as usize);
        (numerator / denominator).clamp(
            ORACLE_PARTITION_WORKING_MEMORY_BYTES,
            ORACLE_PARTITION_MEMORY_BYTES,
        )
    }

    /// Atomically acquires one exact Oracle query envelope.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Occupied`] when aggregate memory,
    /// scratch, slot, or protected interactive capacity cannot cover the exact
    /// request. Invalid or overflowing demand returns
    /// [`BifrostResourceError::InvalidPlan`]. No counter changes on refusal.
    pub(crate) fn try_acquire_oracle(
        &self,
        request: OracleResourceRequest,
        memory_root: &Arc<GovernedMemoryRoot>,
    ) -> Result<OracleQueryResources, BifrostResourceError> {
        let mut state = self.lock_state()?;
        let plan = self.plan();
        Self::validate_oracle_request(plan, request)?;
        // The class quantum is a ceiling and a partition-planning input, not
        // resident memory: what a query actually reserves is charged as its
        // `DataFusion` consumers grow through the shared Oracle root. Slots are
        // the concurrency authority; spill is bytes `DataFusion` actually writes
        // under its own per-query limit, never an admission charge.
        // Slot units are the sole concurrency authority and the sole protector
        // of the Interactive floor. The ledger is shared with follower
        // acquisition, so an Analytical leader and a remote Analytical fragment
        // cannot together spend the units Interactive work is guaranteed.
        let (next_slots, next_analytical_slots) =
            match self.charge_oracle_slots(&state, request.query_class, request.slot_units) {
                Ok(charged) => charged,
                Err(error) => {
                    record_memory_transition("oracle", "refused", state.held(MemoryHolder::Oracle));
                    return Err(error);
                }
            };
        let granted_memory_bytes = Self::oracle_memory_grant(plan, request.slot_units, next_slots);
        let target_partitions = oracle_target_partitions(
            plan.effective_cpu,
            request.local_ratio,
            granted_memory_bytes,
        )?;
        let next_active = state
            .oracle_active_queries
            .checked_add(1)
            .ok_or_else(accounting_overflow)?;
        let next_interactive = state
            .oracle_interactive_queries
            .checked_add(u32::from(request.query_class == QueryClass::Interactive))
            .ok_or_else(accounting_overflow)?;
        let next_analytical = state
            .oracle_analytical_queries
            .checked_add(u32::from(request.query_class == QueryClass::Analytical))
            .ok_or_else(accounting_overflow)?;
        state.oracle_query_slot_units = next_slots;
        state.oracle_analytical_slot_units = next_analytical_slots;
        state.oracle_active_queries = next_active;
        state.oracle_interactive_queries = next_interactive;
        state.oracle_analytical_queries = next_analytical;
        record_memory_transition("oracle", "acquired", state.held(MemoryHolder::Oracle));
        record_oracle_capacity(&state, &plan, self.oracle_class_split());
        let memory_peak_bytes = Arc::new(AtomicUsize::new(0));
        let memory_pool = memory_root.query_view(granted_memory_bytes, &memory_peak_bytes);
        Ok(OracleQueryResources {
            query_class: request.query_class,
            granted_memory_bytes,
            spill_limit_bytes: request.spill_limit_bytes.min(plan.scratch_limit_bytes),
            slot_units: request.slot_units,
            target_partitions,
            memory_pool,
            memory_peak_bytes,
            governor: self.clone(),
            released: false,
            admission_charge: None,
            #[cfg(feature = "test-support")]
            park: None,
        })
    }

    /// Acquires an exact Oracle-role memory owner against the shared cap.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::InvalidPlan`] when Oracle is inactive,
    /// [`BifrostResourceError::Occupied`] when the shared cap cannot cover
    /// `bytes`, and a poison error for an untrustworthy ledger.
    fn try_acquire_oracle_memory(
        &self,
        bytes: usize,
    ) -> Result<OracleMemoryLease, BifrostResourceError> {
        let mut state = self.lock_state()?;
        let plan = self.plan();
        if !plan.oracle_enabled {
            return Err(BifrostResourceError::InvalidPlan {
                detail: "Oracle resources requested while the role is inactive".to_owned(),
            });
        }
        self.charge_locked(&mut state, MemoryHolder::Oracle, bytes)?;
        record_oracle_capacity(&state, &plan, self.oracle_class_split());
        Ok(OracleMemoryLease {
            bytes,
            governor: self.clone(),
            released: false,
        })
    }

    /// Charges `bytes` to `holder` when the shared cap covers them.
    ///
    /// This is the one fallible admission every governed holder uses: it
    /// checks `held + headroom + bytes <= cap` under the state lock and either
    /// records the whole charge or nothing.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Occupied`] without mutation when the
    /// shared cap cannot cover `bytes`, and an overflow poison error when the
    /// checked addition fails.
    fn charge_locked(
        &self,
        state: &mut ResourceState,
        holder: MemoryHolder,
        bytes: usize,
    ) -> Result<(), BifrostResourceError> {
        let next_total = state
            .governed_total()
            .checked_add(bytes)
            .ok_or_else(accounting_overflow)?;
        if next_total > self.plan().managed_memory_bytes {
            record_memory_transition(holder.label(), "refused", state.held(holder));
            return Err(BifrostResourceError::Occupied {
                detail: format!(
                    "{} memory request exceeds the shared Bifrost cap",
                    holder.label()
                ),
            });
        }
        let next = state
            .held(holder)
            .checked_add(bytes)
            .ok_or_else(accounting_overflow)?;
        state.held_bytes[holder as usize] = next;
        record_memory_transition(holder.label(), "acquired", next);
        Ok(())
    }

    /// Returns `bytes` held by `holder` and advances the memory epoch.
    ///
    /// The caller wakes memory waiters after dropping the lock.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Poisoned`] after poisoning the root
    /// when `holder` does not hold `bytes`.
    fn release_locked(
        &self,
        state: &mut ResourceState,
        holder: MemoryHolder,
        bytes: usize,
    ) -> Result<(), BifrostResourceError> {
        let Some(next) = state.held(holder).checked_sub(bytes) else {
            return Err(self.poison_locked(state, "shared memory release underflow"));
        };
        state.held_bytes[holder as usize] = next;
        state.memory_epoch = state.memory_epoch.wrapping_add(1);
        record_memory_transition(holder.label(), "released", next);
        Ok(())
    }

    /// Charges one `DataFusion` pool growth fallibly against the shared cap.
    ///
    /// This is the safety boundary behind every `MemoryPool::try_grow` a
    /// governed pool issues. Oracle growth also updates the query-memory
    /// attribution Oracle capacity telemetry reads.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Occupied`] when the shared cap cannot
    /// cover `bytes`, and a poison error when the ledger is untrustworthy or
    /// the addition overflows. Refusal changes no counter.
    fn try_reserve_pool_memory(
        &self,
        holder: MemoryHolder,
        bytes: usize,
    ) -> Result<GovernedMemoryCharge, BifrostResourceError> {
        let mut state = self.lock_state()?;
        self.charge_locked(&mut state, holder, bytes)?;
        if holder == MemoryHolder::Oracle {
            state.oracle_query_memory_used_bytes = state
                .oracle_query_memory_used_bytes
                .checked_add(bytes)
                .ok_or_else(accounting_overflow)?;
            record_oracle_memory(&state);
        }
        Ok(GovernedMemoryCharge {
            governed_bytes: bytes,
            headroom_bytes: 0,
        })
    }

    /// Charges `DataFusion`'s mandatory infallible growth without refusing.
    ///
    /// The portion that fits both the shared cap and `governed_ceiling` is
    /// charged exactly as a fallible reservation would be. The remainder is
    /// real memory the process now holds, so it is accounted as infallible
    /// headroom instead of being ignored; it counts toward the held total, so
    /// later fallible growth refuses sooner until it is released.
    ///
    /// `governed_ceiling` is the calling pool's remaining grant. A pod with
    /// free capacity must not let one Oracle query's infallible path govern
    /// bytes above that grant, because the grant is what keeps sibling queries
    /// fundable.
    ///
    /// # Errors
    ///
    /// Returns a poison error when the ledger is untrustworthy or the checked
    /// addition overflows.
    fn reserve_pool_memory_infallible(
        &self,
        holder: MemoryHolder,
        bytes: usize,
        governed_ceiling: usize,
    ) -> Result<GovernedMemoryCharge, BifrostResourceError> {
        let mut state = self.lock_state()?;
        let free = self
            .plan()
            .managed_memory_bytes
            .saturating_sub(state.governed_total());
        let governed = bytes.min(free).min(governed_ceiling);
        let headroom = bytes - governed;
        let next = state
            .held(holder)
            .checked_add(governed)
            .ok_or_else(accounting_overflow)?;
        state.infallible_headroom_bytes = state
            .infallible_headroom_bytes
            .checked_add(headroom)
            .ok_or_else(accounting_overflow)?;
        state.held_bytes[holder as usize] = next;
        if holder == MemoryHolder::Oracle {
            state.oracle_query_memory_used_bytes = state
                .oracle_query_memory_used_bytes
                .checked_add(governed)
                .ok_or_else(accounting_overflow)?;
            record_oracle_memory(&state);
        }
        Ok(GovernedMemoryCharge {
            governed_bytes: governed,
            headroom_bytes: headroom,
        })
    }

    /// Returns one exact pool-memory charge and wakes capacity waiters.
    ///
    /// Headroom is released before governed bytes so a consumer that overshot
    /// the cap gives that overshoot back first; only then does governed
    /// capacity reappear for the next fallible reservation.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Poisoned`] and poisons process resource
    /// health when any component of the charge exceeds the retained counters,
    /// because that means two owners believe they hold the same bytes.
    fn release_pool_memory(
        &self,
        holder: MemoryHolder,
        charge: GovernedMemoryCharge,
    ) -> Result<(), BifrostResourceError> {
        if charge.governed_bytes == 0 && charge.headroom_bytes == 0 {
            return Ok(());
        }
        let mut state = self.lock_state()?;
        let query_underflow = holder == MemoryHolder::Oracle
            && state.oracle_query_memory_used_bytes < charge.governed_bytes;
        if state.infallible_headroom_bytes < charge.headroom_bytes || query_underflow {
            return Err(self.poison_locked(&mut state, "pool memory release underflow"));
        }
        self.release_locked(&mut state, holder, charge.governed_bytes)?;
        state.infallible_headroom_bytes -= charge.headroom_bytes;
        if holder == MemoryHolder::Oracle {
            state.oracle_query_memory_used_bytes -= charge.governed_bytes;
            record_oracle_memory(&state);
        }
        drop(state);
        self.inner.memory_changed.notify_waiters();
        Ok(())
    }

    /// Charges exact transport-body bytes against the shared cap.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Occupied`] without mutation when the
    /// shared cap cannot cover `bytes`, or a poison error for an untrustworthy
    /// ledger.
    pub(crate) fn try_charge_transport(&self, bytes: usize) -> Result<(), BifrostResourceError> {
        let mut state = self.lock_state()?;
        self.charge_locked(&mut state, MemoryHolder::Transport, bytes)
    }

    /// Returns exact transport-body bytes and wakes memory waiters.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Poisoned`] when the transport
    /// attribution cannot cover `bytes`.
    pub(crate) fn release_transport(&self, bytes: usize) -> Result<(), BifrostResourceError> {
        let mut state = self.lock_state()?;
        self.release_locked(&mut state, MemoryHolder::Transport, bytes)?;
        drop(state);
        self.inner.memory_changed.notify_waiters();
        Ok(())
    }

    /// Returns bytes currently attributed to transport bodies.
    pub(crate) fn transport_used_bytes(&self) -> usize {
        self.lock_state()
            .map_or(0, |state| state.held(MemoryHolder::Transport))
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, ResourceState>, BifrostResourceError> {
        if let Some(reason) = self.inner.health.reason() {
            return Err(BifrostResourceError::Poisoned {
                detail: format!("resource health entered {reason:?}"),
            });
        }
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| BifrostResourceError::Poisoned {
                detail: "resource state lock is poisoned".to_owned(),
            })?;
        if state.poisoned {
            return Err(BifrostResourceError::Poisoned {
                detail: "a prior release invariant failed".to_owned(),
            });
        }
        Ok(state)
    }

    fn poison_locked(&self, state: &mut ResourceState, detail: &str) -> BifrostResourceError {
        // Log the cause here rather than relying on the returned error: callers
        // on cleanup and Drop paths routinely discard it, which is how the
        // originating imbalance has been lost before.
        tracing::error!(detail, "Bifrost resource accounting failed to reconcile");
        state.poisoned = true;
        state.memory_epoch = state.memory_epoch.wrapping_add(1);
        self.inner
            .health
            .poison(BifrostResourcePoisonReason::Accounting);
        self.inner.memory_changed.notify_waiters();
        self.notify_oracle_capacity();
        BifrostResourceError::Poisoned {
            detail: detail.to_owned(),
        }
    }

    /// Marks the shared allocator untrustworthy after coupled accounting fails.
    pub(crate) fn poison(&self, detail: &'static str) {
        let reason = if detail.contains("runtime") {
            BifrostResourcePoisonReason::RuntimeOwner
        } else {
            BifrostResourcePoisonReason::Accounting
        };
        self.inner.health.poison(reason);
        if let Ok(mut state) = self.inner.state.lock() {
            state.poisoned = true;
            state.memory_epoch = state.memory_epoch.wrapping_add(1);
            self.inner.memory_changed.notify_waiters();
            self.notify_oracle_capacity();
            tracing::error!(detail, "Bifrost resource accounting poisoned");
        } else {
            tracing::error!(detail, "Bifrost resource lock poisoned");
        }
    }
}

/// Move-only root-backed owner of exact Scribe memory and attribution.
#[derive(Debug)]
pub struct ScribeMemoryLease {
    /// Sole process authority that admitted this owner.
    root: BifrostResourceGovernor,
    /// Exact bytes retained by this owner.
    bytes: usize,
    /// Current lifecycle category attributed at the root.
    category: ScribeMemoryCategory,
    /// Optional shard attribution retained across ownership transformations.
    shard: Option<usize>,
    /// Whether ownership has already returned or transferred.
    released: bool,
}

impl ScribeMemoryLease {
    /// Returns the exact bytes retained by this owner.
    #[must_use]
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }

    /// Returns unused bytes from a maintenance owner without reacquiring capacity.
    ///
    /// This is used after a configured-ceiling reservation has protected a
    /// bounded materialization. The surviving owner retains only the exact
    /// materialized live set and preserves its original root/category lineage.
    ///
    /// # Errors
    ///
    /// Returns an invalid-plan error when `bytes` would grow the owner, or a
    /// poison error when the root cannot reconcile the exact shrink.
    pub(crate) fn shrink_to(&mut self, bytes: usize) -> Result<(), BifrostResourceError> {
        if bytes > self.bytes {
            return Err(BifrostResourceError::InvalidPlan {
                detail: "Scribe lease shrink target exceeds owned bytes".to_owned(),
            });
        }
        self.resize_with_limit(bytes, None)
    }

    /// Splits exact bytes into a second owner without new capacity admission.
    ///
    /// # Errors
    ///
    /// Returns an invalid-plan error when `bytes` exceeds this owner.
    pub(crate) fn split(&mut self, bytes: usize) -> Result<Self, BifrostResourceError> {
        if bytes > self.bytes {
            return Err(BifrostResourceError::InvalidPlan {
                detail: "Scribe lease split exceeds owned bytes".to_owned(),
            });
        }
        self.bytes -= bytes;
        Ok(Self {
            root: self.root.clone(),
            bytes,
            category: self.category,
            shard: self.shard,
            released: false,
        })
    }

    /// Merges an identically attributed sibling without changing root totals.
    ///
    /// # Errors
    ///
    /// Returns an invalid-plan error for a foreign root, differing attribution,
    /// or checked byte overflow. The supplied owner remains live on failure.
    pub(crate) fn merge(&mut self, mut other: Self) -> Result<(), BifrostResourceError> {
        if !Arc::ptr_eq(&self.root.inner, &other.root.inner)
            || self.category != other.category
            || self.shard != other.shard
        {
            return Err(BifrostResourceError::InvalidPlan {
                detail: "only sibling Scribe leases with identical attribution may merge"
                    .to_owned(),
            });
        }
        let merged_bytes = self
            .bytes
            .checked_add(other.bytes)
            .ok_or_else(accounting_overflow)?;
        self.bytes = merged_bytes;
        other.released = true;
        other.bytes = 0;
        Ok(())
    }

    /// Resizes this owner without an ingress sublimit for pure ownership tests.
    ///
    /// # Errors
    ///
    /// Returns a typed refusal without mutation when root capacity cannot cover
    /// growth, or a poison error when shrink accounting diverges.
    #[cfg(test)]
    fn resize(&mut self, bytes: usize) -> Result<(), BifrostResourceError> {
        self.resize_with_limit(bytes, None)
    }

    /// Resizes this owner while optionally enforcing an atomic Scribe sublimit.
    ///
    /// Growth performs real shared-cap admission and shrink returns the
    /// exact delta. The optional ingress ceiling is checked under the same root
    /// lock before any capacity or attribution mutation. Every successful size
    /// change advances the capacity epoch before waking waiters.
    ///
    /// # Errors
    ///
    /// Returns a typed refusal without mutation when growth exceeds the supplied
    /// limit or root capacity, or a poison error when shrink accounting diverges.
    fn resize_with_limit(
        &mut self,
        bytes: usize,
        limit_bytes: Option<usize>,
    ) -> Result<(), BifrostResourceError> {
        if bytes == self.bytes {
            return Ok(());
        }
        let mut state = self.root.lock_state()?;
        if bytes > self.bytes {
            self.grow_locked(&mut state, bytes - self.bytes, limit_bytes)?;
        } else {
            self.shrink_locked(&mut state, self.bytes - bytes)?;
        }
        self.bytes = bytes;
        state.memory_epoch = state.memory_epoch.wrapping_add(1);
        drop(state);
        self.root.inner.memory_changed.notify_waiters();
        Ok(())
    }

    /// Applies checked Scribe growth while the sole root state lock is held.
    ///
    /// # Errors
    ///
    /// Returns a capacity refusal or arithmetic error without mutating state.
    fn grow_locked(
        &self,
        state: &mut ResourceState,
        growth: usize,
        limit_bytes: Option<usize>,
    ) -> Result<(), BifrostResourceError> {
        let next_total = state
            .held(MemoryHolder::Scribe)
            .checked_add(growth)
            .ok_or_else(accounting_overflow)?;
        if limit_bytes.is_some_and(|limit| next_total > limit) {
            return Err(BifrostResourceError::Occupied {
                detail: "Scribe resize exceeds the ingress sublimit".to_owned(),
            });
        }
        let category = self.category as usize;
        let next_category = state.scribe_category_bytes[category]
            .checked_add(growth)
            .ok_or_else(accounting_overflow)?;
        let next_shard = self
            .shard
            .map(|shard| {
                state
                    .scribe_shard_bytes
                    .get(&shard)
                    .copied()
                    .unwrap_or_default()
                    .checked_add(growth)
                    .ok_or_else(accounting_overflow)
            })
            .transpose()?;
        self.root
            .charge_locked(state, MemoryHolder::Scribe, growth)?;
        state.scribe_category_bytes[category] = next_category;
        if let (Some(shard), Some(total)) = (self.shard, next_shard) {
            state.scribe_shard_bytes.insert(shard, total);
        }
        Ok(())
    }

    /// Applies checked Scribe shrinkage while the sole root state lock is held.
    ///
    /// # Errors
    ///
    /// Returns a poison error without releasing suspect capacity when any
    /// attribution counter cannot cover the requested shrink.
    fn shrink_locked(
        &self,
        state: &mut ResourceState,
        shrink: usize,
    ) -> Result<(), BifrostResourceError> {
        let category = self.category as usize;
        let shard_total = self.shard.map(|shard| {
            state
                .scribe_shard_bytes
                .get(&shard)
                .copied()
                .unwrap_or_default()
        });
        if state.held(MemoryHolder::Scribe) < shrink
            || state.scribe_category_bytes[category] < shrink
            || shard_total.is_some_and(|total| total < shrink)
        {
            return Err(self
                .root
                .poison_locked(state, "Scribe resize attribution underflow"));
        }
        self.root
            .release_locked(state, MemoryHolder::Scribe, shrink)?;
        state.scribe_category_bytes[category] -= shrink;
        if let Some(shard) = self.shard {
            let remaining = shard_total.unwrap_or_default() - shrink;
            if remaining == 0 {
                state.scribe_shard_bytes.remove(&shard);
            } else {
                state.scribe_shard_bytes.insert(shard, remaining);
            }
        }
        Ok(())
    }

    /// Reclassifies this owner without changing root capacity.
    ///
    /// # Errors
    ///
    /// Returns a poison error when the prior category cannot cover this owner.
    pub(crate) fn reclassify(
        &mut self,
        category: ScribeMemoryCategory,
    ) -> Result<(), BifrostResourceError> {
        if category == self.category {
            return Ok(());
        }
        let mut state = self.root.lock_state()?;
        let prior = self.category as usize;
        let next = category as usize;
        if state.scribe_category_bytes[prior] < self.bytes {
            return Err(self
                .root
                .poison_locked(&mut state, "Scribe category reclassification underflow"));
        }
        let next_bytes = state.scribe_category_bytes[next]
            .checked_add(self.bytes)
            .ok_or_else(accounting_overflow)?;
        state.scribe_category_bytes[prior] -= self.bytes;
        state.scribe_category_bytes[next] = next_bytes;
        self.category = category;
        Ok(())
    }

    /// Reclassifies this owner at the root under the historical lifecycle verb.
    ///
    /// # Errors
    ///
    /// Returns the stable Scribe error projection when attribution diverges.
    pub(crate) fn transfer_category(
        &mut self,
        category: ScribeMemoryCategory,
    ) -> Result<(), crate::contracts::ScribeError> {
        self.reclassify(category).map_err(scribe_resource_error)
    }

    /// Moves this owner's optional shard attribution under the root lock.
    ///
    /// # Errors
    ///
    /// Returns a stable Scribe internal error when prior shard attribution
    /// cannot cover the lease or checked destination arithmetic overflows.
    pub(crate) fn attach_shard(
        &mut self,
        shard: usize,
    ) -> Result<(), crate::contracts::ScribeError> {
        if self.shard == Some(shard) {
            return Ok(());
        }
        let mut state = self.root.lock_state().map_err(scribe_resource_error)?;
        let next = state
            .scribe_shard_bytes
            .get(&shard)
            .copied()
            .unwrap_or_default()
            .checked_add(self.bytes)
            .ok_or_else(|| crate::contracts::ScribeError::Internal {
                detail: "Scribe shard attribution overflowed".to_owned(),
            })?;
        if let Some(prior) = self.shard {
            let prior_bytes = state
                .scribe_shard_bytes
                .get(&prior)
                .copied()
                .unwrap_or_default();
            if prior_bytes < self.bytes {
                return Err(scribe_resource_error(
                    self.root
                        .poison_locked(&mut state, "Scribe shard reattribution underflow"),
                ));
            }
            let remaining = prior_bytes - self.bytes;
            if remaining == 0 {
                state.scribe_shard_bytes.remove(&prior);
            } else {
                state.scribe_shard_bytes.insert(prior, remaining);
            }
        }
        state.scribe_shard_bytes.insert(shard, next);
        self.shard = Some(shard);
        Ok(())
    }

    /// Clears shard identity before joining an aggregate owner.
    ///
    /// # Errors
    ///
    /// Returns a poison error when the prior shard attribution cannot cover
    /// this lease.
    pub(crate) fn clear_identity_attribution(&mut self) -> Result<(), BifrostResourceError> {
        let mut state = self.root.lock_state()?;
        if let Some(shard) = self.shard {
            let prior = state
                .scribe_shard_bytes
                .get(&shard)
                .copied()
                .unwrap_or_default();
            if prior < self.bytes {
                return Err(self
                    .root
                    .poison_locked(&mut state, "Scribe aggregate shard clear underflow"));
            }
            let remaining = prior - self.bytes;
            if remaining == 0 {
                state.scribe_shard_bytes.remove(&shard);
            } else {
                state.scribe_shard_bytes.insert(shard, remaining);
            }
            self.shard = None;
        }
        Ok(())
    }

    /// Resizes ingress ownership through the same root transaction.
    ///
    /// # Errors
    ///
    /// Returns the stable Scribe error projection on role-ceiling overflow,
    /// refusal, or poison.
    pub(crate) fn resize_ingress(
        &mut self,
        bytes: usize,
    ) -> Result<(), crate::contracts::ScribeError> {
        let ingress_limit = self.root.plan().managed_memory_bytes;
        self.resize_with_limit(bytes, Some(ingress_limit))
            .map_err(scribe_resource_error)
    }

    /// Validates that this owner can release `bytes` without mutation.
    ///
    /// # Errors
    ///
    /// Returns a stable internal error and poisons the root on underflow.
    pub(crate) fn preflight_release(
        &self,
        bytes: usize,
    ) -> Result<(), crate::contracts::ScribeError> {
        if self.bytes < bytes {
            self.root.poison("Scribe lease preflight underflow");
            return Err(crate::contracts::ScribeError::Internal {
                detail: "Scribe lease preflight underflow".to_owned(),
            });
        }
        Ok(())
    }

    /// Transfers exact bytes between sibling category owners without admission.
    ///
    /// # Errors
    ///
    /// Returns a stable internal error when roots differ, the source cannot
    /// cover the transfer, or destination arithmetic overflows.
    pub(crate) fn transfer_bytes_to(
        &mut self,
        other: &mut Self,
        bytes: usize,
    ) -> Result<(), crate::contracts::ScribeError> {
        if !Arc::ptr_eq(&self.root.inner, &other.root.inner) || self.bytes < bytes {
            self.root
                .poison("Scribe category transfer ownership mismatch");
            return Err(crate::contracts::ScribeError::Internal {
                detail: "Scribe category transfer ownership mismatch".to_owned(),
            });
        }
        let target = other.bytes.checked_add(bytes).ok_or_else(|| {
            crate::contracts::ScribeError::Internal {
                detail: "Scribe category transfer overflowed".to_owned(),
            }
        })?;
        let mut child = self.split(bytes).map_err(scribe_resource_error)?;
        child.transfer_category(other.category)?;
        other.merge(child).map_err(scribe_resource_error)?;
        debug_assert_eq!(other.bytes, target);
        Ok(())
    }

    /// Poisons the sole root after a lease-level invariant failure.
    pub(crate) fn poison(&self) {
        self.root.poison("Scribe lease invariant failed");
    }

    /// Reports whether this lease's sole root has entered fail-stop health.
    ///
    /// Shard owners use this observation to stop advancing admitted work after
    /// an unresolved durable-control ambiguity while retaining the exact
    /// generation owner for restart reconciliation.
    #[must_use]
    pub(crate) fn is_poisoned(&self) -> bool {
        self.root.inner.health.reason().is_some()
    }

    /// Returns this owner's exact root capacity and attribution once.
    ///
    /// # Errors
    ///
    /// Returns a poison error and retains suspect capacity when any root
    /// projection cannot cover the lease.
    fn release(&mut self) -> Result<(), BifrostResourceError> {
        if self.released {
            return Ok(());
        }
        let mut state = self.root.lock_state()?;
        let category = self.category as usize;
        let shard_bytes = self.shard.map(|shard| {
            state
                .scribe_shard_bytes
                .get(&shard)
                .copied()
                .unwrap_or_default()
        });
        if state.held(MemoryHolder::Scribe) < self.bytes
            || state.scribe_category_bytes[category] < self.bytes
            || shard_bytes.is_some_and(|bytes| bytes < self.bytes)
        {
            return Err(self
                .root
                .poison_locked(&mut state, "Scribe lease release attribution mismatch"));
        }
        self.root
            .release_locked(&mut state, MemoryHolder::Scribe, self.bytes)?;
        state.scribe_category_bytes[category] -= self.bytes;
        if let Some(shard) = self.shard {
            let remaining = shard_bytes.unwrap_or_default() - self.bytes;
            if remaining == 0 {
                state.scribe_shard_bytes.remove(&shard);
            } else {
                state.scribe_shard_bytes.insert(shard, remaining);
            }
        }
        self.released = true;
        drop(state);
        self.root.inner.memory_changed.notify_waiters();
        Ok(())
    }
}

impl Drop for ScribeMemoryLease {
    /// Returns exact root ownership and poisons rather than releasing on mismatch.
    fn drop(&mut self) {
        if let Err(error) = self.release() {
            tracing::error!(%error, "Scribe root lease cleanup failed");
        }
    }
}

/// Move-only owner of aggregate Oracle slot units held outside a query envelope.
///
/// Follower fragments admitted through
/// [`OracleResources::try_acquire_worker`] charge the same class-aware ledger a
/// leader query charges, so releasing on every terminal path — including panic
/// unwind — is what keeps the local Interactive floor honest. Release advances
/// the resource-change epoch and wakes queued leaders.
#[derive(Debug)]
struct OracleSlotCharge {
    /// Scheduling class the units were charged under.
    query_class: QueryClass,
    /// Exact aggregate units owned until release.
    slot_units: u32,
    /// Shared root the units were charged against.
    governor: BifrostResourceGovernor,
    /// Whether the exactly-once release already ran.
    released: bool,
}

impl OracleSlotCharge {
    /// Returns the charged units to the shared ledger exactly once.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Poisoned`] when the shared ledger is
    /// unavailable or either slot counter would underflow.
    fn release(&mut self) -> Result<(), BifrostResourceError> {
        if self.released {
            return Ok(());
        }
        let mut state = self.governor.lock_state()?;
        self.governor
            .release_oracle_slots(&mut state, self.query_class, self.slot_units)?;
        state.memory_epoch = state.memory_epoch.wrapping_add(1);
        BifrostResourceGovernor::advance_oracle_capacity(&mut state);
        self.released = true;
        drop(state);
        self.governor.inner.memory_changed.notify_waiters();
        self.governor.notify_oracle_capacity();
        Ok(())
    }
}

impl Drop for OracleSlotCharge {
    /// Returns the exact slot ownership and poisons on divergence.
    fn drop(&mut self) {
        if let Err(error) = self.release() {
            tracing::error!(%error, "Oracle worker slot cleanup failed");
        }
    }
}

/// Non-cloneable owner of one advertised remote Oracle worker quantum.
#[derive(Debug)]
pub struct OracleWorkerResources {
    /// Aggregate slot-ledger units this follower holds until it is dropped.
    _slots: OracleSlotCharge,
    /// Private view over the shared Oracle root this follower allocates from.
    memory_pool: Arc<dyn MemoryPool>,
    /// Trusted grant the bounded pool was sized from.
    granted_memory_bytes: usize,
    /// Partition ceiling admitted for this worker by that same grant.
    admitted_target_partitions: usize,
}

/// Move-only root-backed owner for one Scribe physical follower quantum.
#[derive(Debug)]
pub struct ScribeFollowerLease {
    /// Typed request identity binding concurrency and memory ownership.
    request_id: RequestId,
    /// Existing Scribe-role bounded concurrency permit.
    _permit: OwnedSemaphorePermit,
    /// Existing Scribe memory lease retained until follower stream termination.
    lease: ScribeMemoryLease,
    /// Exact bounded `DataFusion` pool used by the request-local session.
    pool: Arc<dyn MemoryPool>,
}

impl ScribeFollowerLease {
    /// Returns the exact root-accounted bytes available to the request-local pool.
    #[must_use]
    pub fn memory_bytes(&self) -> usize {
        self.lease.bytes
    }

    /// Returns the typed request identity owning this follower lease.
    #[must_use]
    pub fn request_id(&self) -> &RequestId {
        &self.request_id
    }

    /// Returns the exact bounded pool retained by this lease.
    #[must_use]
    pub fn memory_pool(&self) -> Arc<dyn MemoryPool> {
        Arc::clone(&self.pool)
    }
}

impl OracleWorkerResources {
    /// Returns the shared-root view retained by this worker.
    #[must_use]
    pub fn memory_pool(&self) -> Arc<dyn MemoryPool> {
        Arc::clone(&self.memory_pool)
    }

    /// Returns the trusted grant this worker's execution session is shaped by.
    ///
    /// This is the admitted grant, not the caller's request: a follower derives
    /// its session shape from it so a peer cannot tune the execution it is
    /// served by asking for one.
    #[must_use]
    pub const fn granted_memory_bytes(&self) -> usize {
        self.granted_memory_bytes
    }

    /// Returns the partition ceiling admitted alongside this worker's grant.
    #[must_use]
    pub const fn admitted_target_partitions(&self) -> usize {
        self.admitted_target_partitions
    }
}

/// Focused issuer for the fixed Oracle footer-planning child.
#[derive(Debug, Clone)]
pub struct OracleMetadataResources {
    /// Shared Oracle role authority; callers cannot request arbitrary bytes.
    governor: BifrostResourceGovernor,
}

impl OracleMetadataResources {
    /// Acquires the exact 40 MiB footer slot alongside other Oracle owners.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Occupied`] when the remaining room in
    /// the shared cap cannot cover the fixed slot, or a poison/invalid-plan
    /// error when root accounting or role configuration is not trustworthy.
    pub fn try_acquire_footer_slot(
        &self,
    ) -> Result<OracleFooterSlotResources, BifrostResourceError> {
        let lease = self
            .governor
            .try_acquire_oracle_memory(ORACLE_METADATA_MEMORY_BYTES)?;
        Ok(OracleFooterSlotResources { lease })
    }

    /// Reserves exact retained-metadata bytes against the same Oracle root.
    ///
    /// Distinct from [`Self::try_acquire_footer_slot`] in lifetime, not in
    /// authority: the footer slot is the transient workspace one decode needs,
    /// while this is the ownership of bytes the node intends to keep and share
    /// after that decode returns. Both spend the one Oracle managed-memory
    /// root, which is what stops a decoded-metadata cache from becoming a
    /// second, ungoverned memory pool beside the queries it serves.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Occupied`] when the remaining room in
    /// the shared cap cannot cover `bytes`, or a poison/invalid-plan error
    /// when root accounting or role configuration is not trustworthy.
    pub fn try_reserve_metadata(
        &self,
        bytes: usize,
    ) -> Result<MetadataReservation, BifrostResourceError> {
        let lease = self.governor.try_acquire_oracle_memory(bytes)?;
        Ok(MetadataReservation { lease })
    }
}

/// Non-cloneable ownership of retained decoded-metadata bytes.
///
/// Held by the cache entry and by every borrower of that entry's metadata
/// through one shared `Arc`, so the charge returns to the Oracle root only when
/// the last of them is gone. That coupling is the point: evicting an entry
/// whose decoded metadata a running query still holds must not tell the root
/// those bytes are free, because they are not.
#[derive(Debug)]
pub struct MetadataReservation {
    /// Exact floor-first root-memory ownership for the retained bytes.
    lease: OracleMemoryLease,
}

impl MetadataReservation {
    /// Returns the exact bytes this reservation owns.
    #[must_use]
    pub const fn bytes(&self) -> usize {
        self.lease.bytes
    }
}

/// Non-cloneable owner of the fixed Oracle metadata-planning slot.
#[derive(Debug)]
pub struct OracleFooterSlotResources {
    /// Exact floor-first root-memory ownership for footer planning.
    lease: OracleMemoryLease,
}

impl OracleFooterSlotResources {
    /// Returns the fixed metadata slot size retained by this owner.
    #[must_use]
    pub fn memory_bytes(&self) -> usize {
        self.lease.bytes
    }
}

/// One pool reservation split into governed cap bytes and process headroom.
///
/// `DataFusion` requires an infallible growth path, so a consumer can hold
/// bytes the shared cap did not have room for. Keeping the split on the
/// charge is what makes every byte releasable exactly once: governed bytes
/// return to the holder's attribution, headroom bytes return to the explicit
/// overshoot counter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct GovernedMemoryCharge {
    /// Bytes charged against the shared Bifrost cap.
    governed_bytes: usize,
    /// Bytes held above that capacity through the infallible path.
    headroom_bytes: usize,
}

impl GovernedMemoryCharge {
    /// Returns every byte this charge holds, governed or not.
    const fn total(self) -> usize {
        self.governed_bytes.saturating_add(self.headroom_bytes)
    }
}

/// The one governed `DataFusion` memory root a holder's pools share.
///
/// Oracle has one root for every leader, follower, operator, and exchange
/// reservation; Forge has one for every rewrite attempt. Each competes beneath
/// a single tracked [`FairSpillPool`] sized at the shared cap while the
/// governor keeps the one process ledger, and a per-pool view above it
/// enforces that query's or attempt's own ceiling.
#[derive(Debug)]
pub(crate) struct GovernedMemoryRoot {
    /// Shared spill-fair pool that arbitrates every consumer of this holder.
    pool: Arc<dyn MemoryPool>,
    /// Process ledger charged in lockstep with that pool.
    governor: BifrostResourceGovernor,
    /// Holder whose attribution every charge through this root lands in.
    holder: MemoryHolder,
    /// Cooperative maximum: the shared Bifrost cap.
    limit_bytes: usize,
    /// Serializes whole operations so the two ledgers cannot interleave.
    ///
    /// Held first and for the duration of one operation. The governor's and the
    /// pool's own locks are taken and released inside it, never together, so no
    /// lock order exists to invert.
    operation: Mutex<()>,
}

impl GovernedMemoryRoot {
    /// Builds one holder's shared root at the pod's shared Bifrost cap.
    fn new(governor: BifrostResourceGovernor, holder: MemoryHolder) -> Self {
        let limit_bytes = governor.plan().managed_memory_bytes;
        Self {
            pool: finite_pool(limit_bytes),
            governor,
            holder,
            limit_bytes,
            operation: Mutex::new(()),
        }
    }

    /// Returns the immutable cooperative maximum this root arbitrates.
    pub(crate) const fn limit_bytes(&self) -> usize {
        self.limit_bytes
    }

    /// Returns the aggregate bytes every live Oracle consumer holds.
    pub(crate) fn reserved(&self) -> usize {
        self.pool.reserved()
    }

    /// Issues one query's private view over this shared root.
    ///
    /// The view owns only a ceiling and its own consumer ledger; it allocates
    /// nothing of its own, so two views can never sum above the root.
    fn query_view(
        self: &Arc<Self>,
        ceiling_bytes: usize,
        peak_bytes: &Arc<AtomicUsize>,
    ) -> Arc<dyn MemoryPool> {
        Arc::new(GovernedMemoryView {
            root: Arc::clone(self),
            ceiling_bytes,
            ledger: Mutex::new(GovernedMemoryLedger::default()),
            peak_bytes: Arc::clone(peak_bytes),
        })
    }

    /// Locks one whole operation, refusing rather than panicking on poison.
    fn lock_operation(&self) -> Result<std::sync::MutexGuard<'_, ()>, DataFusionError> {
        self.operation.lock().map_err(|_| {
            DataFusionError::ResourcesExhausted(
                "Oracle shared memory root lock is poisoned".to_owned(),
            )
        })
    }

    /// Grows one consumer fallibly through query, governor, and pool in order.
    ///
    /// Refusal happens before any mutation when the query ceiling cannot cover
    /// the request; a later refusal from the governor or the shared pool rolls
    /// the completed steps back in reverse order, so a refused growth retains
    /// no bytes anywhere.
    ///
    /// # Errors
    ///
    /// Returns `DataFusion` resource exhaustion when the query ceiling, the
    /// governed process root, or the shared pool cannot cover `additional`.
    fn try_grow(
        &self,
        view: &GovernedMemoryView,
        reservation: &MemoryReservation,
        additional: usize,
    ) -> Result<(), DataFusionError> {
        let _operation = self.lock_operation()?;
        let mut ledger = view.lock_ledger()?;
        let next_total = ledger.total.checked_add(additional).ok_or_else(|| {
            DataFusionError::ResourcesExhausted("Oracle query memory overflowed".to_owned())
        })?;
        if next_total > view.ceiling_bytes {
            return Err(DataFusionError::ResourcesExhausted(format!(
                "Oracle query memory ceiling of {} bytes exhausted",
                view.ceiling_bytes
            )));
        }
        let charge = self
            .governor
            .try_reserve_pool_memory(self.holder, additional)
            .map_err(|error| root_growth_refusal(&error))?;
        if let Err(error) = self.pool.try_grow(reservation, additional) {
            // Reverse order: the governor charge is the only completed step.
            let _ = self.governor.release_pool_memory(self.holder, charge);
            return Err(error);
        }
        ledger.charge(reservation.consumer().id(), charge);
        ledger.total = next_total;
        view.observe(ledger.total);
        Ok(())
    }

    /// Grows one consumer through the path `DataFusion` does not let fail.
    ///
    /// Bytes above the governed root, or above this query's remaining ceiling,
    /// are still resident, so the governor records them as this consumer's
    /// headroom charge rather than dropping them. The ceiling bound is what
    /// keeps an idle pod from letting one query govern more than its immutable
    /// grant. The ledger keeps the same split, so a query that overshoots here
    /// is refused any further fallible growth and gives its headroom back first
    /// on shrink.
    fn grow(&self, view: &GovernedMemoryView, reservation: &MemoryReservation, additional: usize) {
        let Ok(_operation) = self.lock_operation() else {
            return;
        };
        let Ok(mut ledger) = view.lock_ledger() else {
            return;
        };
        let remaining_ceiling = view.ceiling_bytes.saturating_sub(ledger.governed);
        let charge = match self.governor.reserve_pool_memory_infallible(
            self.holder,
            additional,
            remaining_ceiling,
        ) {
            Ok(charge) => charge,
            Err(error) => {
                tracing::error!(%error, "Oracle infallible growth could not be accounted");
                return;
            }
        };
        self.pool.grow(reservation, additional);
        ledger.charge(reservation.consumer().id(), charge);
        ledger.total = ledger.total.saturating_add(additional);
        view.observe(ledger.total);
    }

    /// Releases bytes from the shared pool, the governor, and the query ledger.
    ///
    /// This is the only callback that returns successful bytes. Headroom is
    /// released before governed bytes so an overshooting consumer gives back
    /// its overshoot first.
    fn shrink(&self, view: &GovernedMemoryView, reservation: &MemoryReservation, shrink: usize) {
        let Ok(_operation) = self.lock_operation() else {
            return;
        };
        let Ok(mut ledger) = view.lock_ledger() else {
            return;
        };
        self.pool.shrink(reservation, shrink);
        let released = ledger.release(reservation.consumer().id(), shrink);
        if let Err(error) = self.governor.release_pool_memory(self.holder, released) {
            tracing::error!(%error, "Oracle memory release could not be reconciled");
        }
        ledger.total = ledger.total.saturating_sub(shrink);
    }
}

/// Per-consumer and aggregate bytes one query view currently holds.
#[derive(Debug, Default)]
struct GovernedMemoryLedger {
    /// Aggregate bytes held by this query, checked against its ceiling.
    total: usize,
    /// Bytes of `total` charged against the governed Oracle root.
    ///
    /// Infallible growth bounds its governed component by the ceiling this
    /// counter has left, so the remainder becomes explicit process headroom.
    governed: usize,
    /// Governed/headroom split per process-unique `MemoryConsumer::id()`.
    consumers: BTreeMap<usize, GovernedMemoryCharge>,
}

impl GovernedMemoryLedger {
    /// Adds one accepted charge to a consumer's retained split.
    fn charge(&mut self, consumer: usize, charge: GovernedMemoryCharge) {
        self.governed = self.governed.saturating_add(charge.governed_bytes);
        let entry = self.consumers.entry(consumer).or_default();
        entry.governed_bytes = entry.governed_bytes.saturating_add(charge.governed_bytes);
        entry.headroom_bytes = entry.headroom_bytes.saturating_add(charge.headroom_bytes);
    }

    /// Removes `bytes` from one consumer, headroom first, and reports the split.
    fn release(&mut self, consumer: usize, bytes: usize) -> GovernedMemoryCharge {
        let Some(entry) = self.consumers.get_mut(&consumer) else {
            return GovernedMemoryCharge::default();
        };
        let headroom = entry.headroom_bytes.min(bytes);
        let governed = entry.governed_bytes.min(bytes - headroom);
        entry.headroom_bytes -= headroom;
        entry.governed_bytes -= governed;
        self.governed = self.governed.saturating_sub(governed);
        if entry.total() == 0 {
            self.consumers.remove(&consumer);
        }
        GovernedMemoryCharge {
            governed_bytes: governed,
            headroom_bytes: headroom,
        }
    }
}

/// One query's private view over the shared Oracle memory root.
///
/// It owns a ceiling and a ledger, never capacity: every registration and
/// reservation is forwarded to the root so the shared pool can arbitrate
/// fairly across all live queries while this view refuses anything past this
/// query's own grant.
#[derive(Debug)]
struct GovernedMemoryView {
    /// Shared root that owns the pool and the process ledger.
    root: Arc<GovernedMemoryRoot>,
    /// Immutable maximum governed bytes this query may hold.
    ceiling_bytes: usize,
    /// This query's consumer split and aggregate counter.
    ledger: Mutex<GovernedMemoryLedger>,
    /// Query-local observed peak, read by capacity journeys.
    peak_bytes: Arc<AtomicUsize>,
}

impl GovernedMemoryView {
    /// Locks this view's ledger, refusing rather than panicking on poison.
    fn lock_ledger(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, GovernedMemoryLedger>, DataFusionError> {
        self.ledger.lock().map_err(|_| {
            DataFusionError::ResourcesExhausted("Oracle query memory ledger is poisoned".to_owned())
        })
    }

    /// Records the query-local peak after a successful growth operation.
    fn observe(&self, total: usize) {
        self.peak_bytes
            .fetch_max(total, std::sync::atomic::Ordering::AcqRel);
    }
}

impl std::fmt::Display for GovernedMemoryView {
    /// Renders the pool name `DataFusion` reports in exhaustion errors.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("oracle_shared_root")
    }
}

impl MemoryPool for GovernedMemoryView {
    /// Returns the stable pool name `DataFusion` attributes reservations to.
    fn name(&self) -> &'static str {
        "oracle_shared_root"
    }

    /// Registers this query's consumer with the shared root unchanged.
    fn register(&self, consumer: &MemoryConsumer) {
        self.root.pool.register(consumer);
    }

    /// Forwards removal; sibling consumers may still hold bytes.
    fn unregister(&self, consumer: &MemoryConsumer) {
        self.root.pool.unregister(consumer);
        if let Ok(mut ledger) = self.lock_ledger()
            && ledger
                .consumers
                .get(&consumer.id())
                .is_none_or(|charge| charge.total() == 0)
        {
            ledger.consumers.remove(&consumer.id());
        }
    }

    /// Charges infallible growth to the query, governor, and shared pool.
    fn grow(&self, reservation: &MemoryReservation, additional: usize) {
        self.root.grow(self, reservation, additional);
    }

    /// Returns bytes to the shared pool, the governor, and this query.
    fn shrink(&self, reservation: &MemoryReservation, shrink: usize) {
        self.root.shrink(self, reservation, shrink);
    }

    /// Refuses at this query's ceiling or at the governed process root.
    fn try_grow(
        &self,
        reservation: &MemoryReservation,
        additional: usize,
    ) -> datafusion::error::Result<()> {
        self.root.try_grow(self, reservation, additional)
    }

    /// Returns every byte this query holds, governed or headroom.
    fn reserved(&self) -> usize {
        self.lock_ledger().map_or(0, |ledger| ledger.total)
    }

    /// Reports the immutable maximum governed allocation for this query.
    fn memory_limit(&self) -> MemoryLimit {
        MemoryLimit::Finite(self.ceiling_bytes)
    }
}

impl Drop for GovernedMemoryView {
    /// Fails the process closed when a query view is dropped holding bytes.
    fn drop(&mut self) {
        let held = self.ledger.lock().map_or(0, |ledger| ledger.total);
        if held != 0 {
            self.root
                .governor
                .poison("Oracle query memory view dropped while holding bytes");
        }
    }
}

/// Projects a governed-root refusal onto `DataFusion`'s typed exhaustion error.
fn root_growth_refusal(error: &BifrostResourceError) -> DataFusionError {
    DataFusionError::ResourcesExhausted(error.to_string())
}

/// Internal exact Oracle shared-cap lease shared by the two public shapes.
#[derive(Debug)]
struct OracleMemoryLease {
    /// Total Oracle bytes owned by this lease.
    bytes: usize,
    /// Root ledger that issued the ownership.
    governor: BifrostResourceGovernor,
    /// Whether exact ownership has already returned to the root.
    released: bool,
}

impl OracleMemoryLease {
    /// Returns exact counters once and poisons on any ownership mismatch.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Poisoned`] when the retained Oracle
    /// attribution cannot cover this lease.
    fn release(&mut self) -> Result<(), BifrostResourceError> {
        if self.released {
            return Ok(());
        }
        let mut state = self.governor.lock_state()?;
        let plan = self.governor.plan();
        self.governor
            .release_locked(&mut state, MemoryHolder::Oracle, self.bytes)?;
        record_oracle_capacity(&state, &plan, self.governor.oracle_class_split());
        self.released = true;
        drop(state);
        self.governor.inner.memory_changed.notify_waiters();
        Ok(())
    }
}

impl Drop for OracleMemoryLease {
    /// Returns the exact shared-cap ownership and poisons on divergence.
    fn drop(&mut self) {
        if let Err(error) = self.release() {
            tracing::error!(%error, "Oracle role resource cleanup failed");
        }
    }
}

/// Query-lifetime Oracle memory, slot, and adaptive parallelism owner.
#[derive(Debug)]
pub struct OracleQueryResources {
    /// Scheduling class charged by this query owner.
    query_class: QueryClass,
    /// Ceiling this query's view of the shared Oracle root may grow to.
    ///
    /// Derived once at admission by
    /// [`BifrostResourceGovernor::oracle_memory_grant`] and held for the query's
    /// life. Admission charges no memory of its own: this is the maximum the
    /// query's actual consumer growth may govern, never a resident reservation.
    pub granted_memory_bytes: usize,
    /// Most bytes `DataFusion` may spill for this query: the class spill
    /// quantum clamped to the resolved scratch limit. Nothing is charged for
    /// it; an unspilled query owns no disk.
    pub spill_limit_bytes: u64,
    /// Exact slot units retained by this query owner.
    slot_units: u32,
    /// Query-local `DataFusion` target partition count.
    pub target_partitions: usize,
    /// One shared pool used by `DataFusion` and every query-owned Wyrd consumer.
    memory_pool: Arc<dyn MemoryPool>,
    /// Largest reservation this query's own pool has held, in bytes.
    ///
    /// Query-local rather than process-global: a journey that runs several
    /// queries concurrently needs each one's peak attributable to the grant it
    /// was admitted under. Only the observing pool wrapper writes it, so it
    /// stays zero on a build without `test-support`.
    memory_peak_bytes: Arc<AtomicUsize>,
    governor: BifrostResourceGovernor,
    released: bool,
    /// Leader-local admission charge returned with this owner's slots.
    ///
    /// Opaque on purpose: the governor never inspects it, it only drops it
    /// after the slot units return and before the capacity wake, so a queued
    /// leader woken by that wake sees the slot and its tenant charge free
    /// together. A remote follower's owner carries none.
    admission_charge: Option<Box<dyn std::any::Any + Send + Sync>>,
    /// Test-tier park this query claimed at admission, if a journey armed one.
    #[cfg(feature = "test-support")]
    pub(crate) park: Option<Arc<OracleQueryPark>>,
}

impl OracleQueryResources {
    /// Attaches the leader-local admission charge this owner returns on release.
    ///
    /// Oracle admission calls this once, under its own lock, immediately after
    /// the governor grants the slot charge. The charge's drop is its release:
    /// it runs exactly once, after the slot units return and before queued
    /// admission is woken. A later call replaces and drops the earlier charge.
    pub(crate) fn attach_admission_charge(&mut self, charge: Box<dyn std::any::Any + Send + Sync>) {
        self.admission_charge = Some(charge);
    }

    /// Builds the tracked first-come, first-served query-local `DataFusion` pool.
    #[must_use]
    pub fn memory_pool(&self) -> Arc<dyn MemoryPool> {
        Arc::clone(&self.memory_pool)
    }

    /// Returns the query-local counter the observing pool records peaks into.
    ///
    /// The counter is shared, not copied: an owner that transfers this envelope
    /// away still names the same peak the pool keeps updating.
    #[must_use]
    pub fn memory_peak_bytes(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.memory_peak_bytes)
    }

    /// Reports whether every nested memory child of this query envelope is gone.
    ///
    /// [`OracleQueryResources::release`] poisons the process governor when a
    /// child outlives its owner, which is correct for a leak but wrong for a
    /// teardown that is merely still in progress. An owner that cannot observe
    /// its consumers directly — a follower whose stage plan is dropped by
    /// upstream's own task cache — asks this first and waits, so the poison
    /// keeps its meaning.
    #[must_use]
    pub fn nested_idle(&self) -> bool {
        self.nested_memory_bytes() == 0
    }

    /// Returns the query-pool memory a child still holds, in bytes.
    ///
    /// A drain that times out is only actionable if it names what stayed.
    #[must_use]
    pub fn nested_memory_bytes(&self) -> usize {
        self.memory_pool.reserved()
    }

    /// Splits one named memory child from the already admitted query pool.
    ///
    /// # Errors
    ///
    /// Returns `DataFusion` resource exhaustion when sibling children have
    /// consumed the query owner. This never admits capacity at the root again.
    pub fn try_split_memory(
        &self,
        consumer: &'static str,
        bytes: usize,
    ) -> Result<OracleQueryMemoryReservation, DataFusionError> {
        OracleQueryMemoryReservation::try_new(
            &self.memory_pool,
            self.governor.clone(),
            consumer,
            bytes,
        )
    }

    /// Releases the query envelope only after every nested child is gone.
    ///
    /// Slot units return under the governor lock; the attached admission
    /// charge is then dropped outside it, and only after both is the Oracle
    /// capacity wake sent, so queued admission is reconsidered once with the
    /// whole query's capacity already free.
    ///
    /// # Errors
    ///
    /// Returns a poison error while retaining root capacity when nested memory
    /// ownership survives, or when root counters diverge.
    fn release(&mut self) -> Result<(), BifrostResourceError> {
        if self.released {
            return Ok(());
        }
        if self.memory_pool.reserved() != 0 {
            self.governor
                .poison("Oracle query owner outlived a nested resource child");
            return Err(BifrostResourceError::Poisoned {
                detail: "Oracle query nested resource child survived owner release".to_owned(),
            });
        }
        let mut state = self.governor.lock_state()?;
        let class_count = match self.query_class {
            QueryClass::Interactive => state.oracle_interactive_queries,
            QueryClass::Analytical => state.oracle_analytical_queries,
        };
        // Query memory is released by the shared root as each consumer shrinks,
        // so this owner returns only what it actually charged: slots and the
        // class counters.
        if state.oracle_active_queries == 0
            || class_count == 0
            || state.oracle_query_slot_units < self.slot_units
        {
            return Err(self
                .governor
                .poison_locked(&mut state, "Oracle query release underflow"));
        }
        state.oracle_active_queries -= 1;
        match self.query_class {
            QueryClass::Interactive => state.oracle_interactive_queries -= 1,
            QueryClass::Analytical => state.oracle_analytical_queries -= 1,
        }
        self.governor
            .release_oracle_slots(&mut state, self.query_class, self.slot_units)?;
        state.memory_epoch = state.memory_epoch.wrapping_add(1);
        BifrostResourceGovernor::advance_oracle_capacity(&mut state);
        record_memory_transition("oracle", "released", state.held(MemoryHolder::Oracle));
        record_oracle_capacity(
            &state,
            &self.governor.plan(),
            self.governor.oracle_class_split(),
        );
        self.released = true;
        drop(state);
        // After the slots return and before the wake: the admission charge
        // takes the admission lock, which is ordered before the governor lock,
        // so it must not run under the state lock above; and the wake below is
        // what reconsiders queued work, so the charge must already be free.
        drop(self.admission_charge.take());
        self.governor.inner.memory_changed.notify_waiters();
        self.governor.notify_oracle_capacity();
        Ok(())
    }
}

/// Query-local ownership registered in the same pool as `DataFusion` operators.
///
/// The reservation is a nested accounting owner inside an already-admitted
/// pod envelope. It never charges the pod governor a second time.
#[derive(Debug)]
pub struct OracleQueryMemoryReservation {
    reservation: MemoryReservation,
    governor: BifrostResourceGovernor,
}

impl OracleQueryMemoryReservation {
    /// Registers one named query consumer and reserves its exact byte owner.
    ///
    /// # Errors
    ///
    /// Returns `DataFusion`'s typed resource-exhaustion error when the aggregate
    /// query pool cannot grow by `bytes`; failed growth retains no bytes.
    pub(crate) fn try_new(
        pool: &Arc<dyn MemoryPool>,
        governor: BifrostResourceGovernor,
        consumer: &'static str,
        bytes: usize,
    ) -> Result<Self, DataFusionError> {
        let reservation = MemoryConsumer::new(consumer).register(pool);
        reservation.try_grow(bytes)?;
        Ok(Self {
            reservation,
            governor,
        })
    }

    /// Returns the exact bytes retained by this query-local owner.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.reservation.size()
    }

    /// Fails closed when telemetry and query-pool ownership diverge.
    pub fn poison(&self) {
        self.governor
            .poison("Oracle query memory telemetry diverged");
    }
}

impl Drop for OracleQueryResources {
    /// Releases the complete envelope exactly once and poisons on corruption.
    fn drop(&mut self) {
        if let Err(error) = self.release() {
            tracing::error!(%error, "Oracle query resource cleanup failed");
        }
    }
}

/// Computes the ceiling on execution partitions an admitted query may use.
///
/// Parallelism is driven by CPU and by how much of the pinned input is remote:
/// a fully remote scan overlaps IO latency across up to four partitions per core,
/// while a fully local scan stays at one partition per core because there is no
/// latency to hide. Memory participates only as a downward clamp, using
/// [`ORACLE_PARTITION_WORKING_MEMORY_BYTES`] — the memory one partition needs to
/// run — rather than the whole-query envelope quantum. Clamping by the envelope
/// quantum would collapse every Interactive query to a single serial partition.
///
/// The result is a ceiling, not a final value. Callers narrow it further by the
/// work actually available in the pinned cut; see
/// [`oracle_partitions_for_work`].
///
/// # Errors
///
/// Returns [`BifrostResourceError::InvalidPlan`] for zero CPU or non-finite or
/// out-of-range locality, and an overflow error for checked `cpu * 4` overflow.
pub fn oracle_target_partitions(
    effective_cpu: usize,
    local_ratio: f64,
    memory_bytes: usize,
) -> Result<usize, BifrostResourceError> {
    if effective_cpu == 0 || !local_ratio.is_finite() || !(0.0..=1.0).contains(&local_ratio) {
        return Err(BifrostResourceError::InvalidPlan {
            detail: "Oracle partition inputs must have positive CPU and locality in [0, 1]"
                .to_owned(),
        });
    }
    let query_threads = effective_cpu
        .checked_mul(4)
        .ok_or_else(accounting_overflow)?;
    let remote_span = query_threads - effective_cpu;
    let remote_partitions = remote_span
        .to_f64()
        .map(|span| (span * (1.0 - local_ratio)).trunc())
        .and_then(|partitions| partitions.to_usize())
        .ok_or_else(accounting_overflow)?;
    let locality = effective_cpu
        .checked_add(remote_partitions)
        .ok_or_else(accounting_overflow)?;
    let memory = memory_bytes / ORACLE_PARTITION_WORKING_MEMORY_BYTES;
    Ok(locality.min(memory).max(ORACLE_MIN_TARGET_PARTITIONS))
}

/// Ceiling on the `DataFusion` batch size an Oracle query may receive.
///
/// This is `DataFusion`'s own default, and it is a ceiling rather than the
/// full-grant value: batches are sized from the memory *one partition* may
/// hold, so only a single-partition query near the grant cap approaches it.
pub const ORACLE_MAX_BATCH_SIZE: usize = 8_192;
/// Smallest `DataFusion` batch size an Oracle query at the floor grant receives.
///
/// Below this, per-batch overhead dominates and the query loses more to task
/// bookkeeping than it saves in memory.
pub const ORACLE_MIN_BATCH_SIZE: usize = 1_024;

/// Fraction of a partition's share of the grant held back for sort merging.
///
/// A `SortExec` that spills reads its runs back through `ExternalSorterMerge`,
/// whose reservation cannot spill: if the sorting partitions have already
/// consumed the pool, the merge fails the whole query with resource exhaustion
/// instead of completing on disk. `DataFusion`'s own default reservation is a
/// fixed 10 MiB, which is unrelated to the grant Bifrost actually admitted, so
/// the reservation is derived from the grant here. Half of each partition's
/// share leaves the sort real working memory while guaranteeing every partition
/// can merge what it spilled.
const SORT_MERGE_RESERVATION_DIVISOR: usize = 2;

/// `DataFusion` session shape derived from one admitted memory grant.
///
/// All three knobs come from a single admission decision on purpose. They are
/// not independent tuning parameters: partitions divide the grant, batch size
/// sets how much each partition holds at once, and the join preference decides
/// whether the plan may contain an operator that cannot survive a small grant.
/// Recomputing any of them at a second call site would let them disagree about
/// how much memory the query actually has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OracleSessionShape {
    /// `DataFusion` target partition count for this query.
    pub target_partitions: usize,
    /// `DataFusion` batch size for this query.
    pub batch_size: usize,
    /// Whether the optimizer may prefer a hash join for this query.
    pub prefer_hash_join: bool,
    /// Per-partition memory a spilling sort holds back for its merge phase.
    pub sort_spill_reservation_bytes: usize,
}

impl OracleSessionShape {
    /// Derives every session knob from one grant and the pinned cut's work.
    ///
    /// Partitions narrow to the work actually available, batch size scales
    /// linearly with one partition's working share of the grant between
    /// [`ORACLE_MIN_BATCH_SIZE`] and [`ORACLE_MAX_BATCH_SIZE`], and hash joins
    /// are disabled once the grant is near the floor.
    ///
    /// Batch size is derived per partition, not from the whole grant, because
    /// the memory a batch costs is paid `target_partitions` times over and the
    /// operators that pay it cannot spill. A spilling `SortExec` converts its
    /// in-memory run into an unspillable merge reservation and
    /// `SortPreservingMergeExec` buffers one batch per partition on top of it;
    /// sizing batches from the whole grant lets those unspillable buffers
    /// exceed the pool, which fails the query outright instead of completing on
    /// disk. The reservation this shape holds back is excluded from the batch
    /// budget for the same reason: it is memory the query has already promised
    /// to the merge.
    ///
    /// The join preference is the load-bearing one. `HashJoinExec` cannot spill:
    /// it grows a reservation and returns resource exhaustion when the grant is
    /// too small, where sort, grouped aggregate, and sort-merge join all spill to
    /// disk and complete. Shrinking a query's ceiling under concurrency is only
    /// safe *because* a small grant also routes the plan away from the one
    /// operator that would hard-fail instead of spilling. Without this the
    /// dynamic grant converts a clean refusal into a failed query.
    #[must_use]
    pub fn for_grant(
        granted_memory_bytes: usize,
        admitted_partitions: usize,
        work_units: usize,
    ) -> Self {
        let target_partitions = oracle_partitions_for_work(admitted_partitions, work_units);
        let partition_share = granted_memory_bytes / target_partitions.max(1);
        let sort_spill_reservation_bytes = partition_share / SORT_MERGE_RESERVATION_DIVISOR;
        let scaled = ORACLE_MAX_BATCH_SIZE
            .saturating_mul(partition_share.saturating_sub(sort_spill_reservation_bytes))
            / ORACLE_PARTITION_MEMORY_BYTES.max(1);
        let batch_size = scaled.clamp(ORACLE_MIN_BATCH_SIZE, ORACLE_MAX_BATCH_SIZE);
        let prefer_hash_join =
            granted_memory_bytes > ORACLE_PARTITION_WORKING_MEMORY_BYTES.saturating_mul(2);
        Self {
            target_partitions,
            batch_size,
            prefer_hash_join,
            sort_spill_reservation_bytes,
        }
    }

    /// Builds the `DataFusion` session configuration this shape describes.
    ///
    /// This is the only place the three knobs reach `DataFusion`, so a caller
    /// cannot apply two of them and silently drop the third.
    #[must_use]
    pub fn session_config(self) -> datafusion::execution::context::SessionConfig {
        self.apply(datafusion::execution::context::SessionConfig::new())
    }

    /// Applies every grant-derived knob to a session an Oracle will execute in.
    ///
    /// A closed leaf predicate recognized by `OracleTableProvider`'s classifier
    /// only prunes files, row groups, and pages if the `DataFusion` session that
    /// actually opens the Parquet files enables pushdown, reorders filters ahead
    /// of decoding, and consults bloom filters/page indexes. The memory knobs
    /// travel with them because a stage that reads the leader's plan under
    /// `DataFusion`'s own defaults holds far larger batches than the grant was
    /// sized for, and the operators that hold them cannot spill. The leader's
    /// query-execution session ([`Self::session_config`]), the follower's
    /// per-request dispatch session (`FollowerSessionFactory::create`), and a
    /// distributed stage session all route through this one function rather
    /// than setting knobs inline, so no path can silently drift onto defaults.
    #[must_use]
    pub fn apply(
        &self,
        config: datafusion::execution::context::SessionConfig,
    ) -> datafusion::execution::context::SessionConfig {
        let mut config = config
            .with_target_partitions(self.target_partitions)
            .with_batch_size(self.batch_size);
        let options = config.options_mut();
        options.optimizer.prefer_hash_join = self.prefer_hash_join;
        options.execution.sort_spill_reservation_bytes = self.sort_spill_reservation_bytes;
        let parquet_options = &mut options.execution.parquet;
        parquet_options.pushdown_filters = true;
        parquet_options.reorder_filters = true;
        parquet_options.bloom_filter_on_read = true;
        parquet_options.enable_page_index = true;
        config
    }
}

/// Narrows an admitted partition ceiling to the work the pinned cut actually has.
///
/// Splitting a two-file scan across sixteen partitions costs more in task setup
/// and empty-stream merging than it recovers in parallelism, so parallelism is
/// capped at one partition per scannable unit. The floor still applies, so a
/// single-file query keeps [`ORACLE_MIN_TARGET_PARTITIONS`] rather than
/// collapsing to a serial plan.
///
/// A zero work count means the cut pinned nothing scannable; the query still
/// receives the floor so its empty plan executes normally.
#[must_use]
pub fn oracle_partitions_for_work(admitted_ceiling: usize, work_units: usize) -> usize {
    admitted_ceiling
        .min(work_units.max(ORACLE_MIN_TARGET_PARTITIONS))
        .max(ORACLE_MIN_TARGET_PARTITIONS)
}

/// Builds a finite spill-fair pool for a nested resource envelope.
///
/// A query now runs across many execution partitions. A first-come pool lets one
/// partition claim the whole envelope while its peers are starved into spilling
/// early. [`FairSpillPool`] divides the envelope evenly across live spillable
/// reservations, so each partition gets a predictable share and operators spill
/// only when the query as a whole is genuinely out of memory.
///
/// The caller must retain the outer [`BifrostResourceGovernor`] lease for at
/// least as long as this pool can be referenced. A zero limit is normalized to
/// one byte only for defensive construction; production admission never grants
/// a zero-byte envelope.
///
/// Oracle queries no longer use this: they share one governed root instead.
/// It remains for the fixtures that still need a standalone finite envelope.
#[cfg(test)]
#[must_use]
pub(crate) fn bounded_memory_pool(limit_bytes: usize) -> Arc<dyn MemoryPool> {
    finite_pool(limit_bytes)
}

/// Builds the production finite pool every Bifrost owner allocates from.
fn finite_pool(limit_bytes: usize) -> Arc<dyn MemoryPool> {
    Arc::new(TrackConsumersPool::new(
        FairSpillPool::new(limit_bytes.max(1)),
        NonZeroUsize::new(16).unwrap_or(NonZeroUsize::MIN),
    ))
}

fn cap_positive(
    detected: usize,
    override_value: Option<usize>,
    name: &str,
) -> Result<usize, BifrostResourceError> {
    if detected == 0 {
        return Err(BifrostResourceError::Unavailable {
            detail: format!("no positive {name} bound was detected"),
        });
    }
    let resolved = override_value.map_or(detected, |value| value.min(detected));
    if resolved == 0 {
        return Err(BifrostResourceError::InvalidPlan {
            detail: format!("{name} override must be positive"),
        });
    }
    Ok(resolved)
}

fn accounting_overflow() -> BifrostResourceError {
    BifrostResourceError::Poisoned {
        detail: "resource accounting overflow".to_owned(),
    }
}

/// Projects private root refusals onto the stable Scribe behavior boundary.
fn scribe_resource_error(error: BifrostResourceError) -> crate::contracts::ScribeError {
    match error {
        BifrostResourceError::Occupied { .. } => crate::contracts::ScribeError::IngestBusy {
            table: "memory".to_owned(),
        },
        error => crate::contracts::ScribeError::Internal {
            detail: error.to_string(),
        },
    }
}

fn detect_snapshot(
    scratch_root: &Path,
    memory_override: Option<usize>,
) -> Result<SystemResourceSnapshot, BifrostResourceError> {
    let (host_memory, host_source) = match host_memory_limit() {
        Ok(detected) => detected,
        Err(_) => (
            memory_override.ok_or_else(|| BifrostResourceError::Unavailable {
                detail: "host memory probe is unavailable; set WYRD_BIFROST_MEMORY_LIMIT_BYTES"
                    .to_owned(),
            })?,
            ResourceSource::Override,
        ),
    };
    let cgroup_memory = read_positive_usize("/sys/fs/cgroup/memory.max")
        .map(|value| (value, ResourceSource::CgroupV2))
        .or_else(|| {
            read_positive_usize("/sys/fs/cgroup/memory/memory.limit_in_bytes")
                .map(|value| (value, ResourceSource::CgroupV1))
        });
    let (memory_limit_bytes, memory_source) = cgroup_memory
        .filter(|(value, _)| *value < host_memory)
        .unwrap_or((host_memory, host_source));
    let available = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .map_err(|error| BifrostResourceError::Unavailable {
            detail: format!("available CPU detection failed: {error}"),
        })?;
    let (effective_cpu, cpu_source) = detect_cgroup_cpu()
        .filter(|value| *value < available)
        .map_or((available, ResourceSource::Host), |value| {
            (value, ResourceSource::CgroupV2)
        });
    let stats = statvfs(scratch_root).map_err(|error| BifrostResourceError::Unavailable {
        detail: format!("scratch filesystem inspection failed: {error}"),
    })?;
    let block_size = if stats.f_frsize == 0 {
        stats.f_bsize
    } else {
        stats.f_frsize
    };
    Ok(SystemResourceSnapshot {
        memory_limit_bytes,
        effective_cpu,
        scratch_capacity_bytes: stats
            .f_blocks
            .checked_mul(block_size)
            .ok_or_else(accounting_overflow)?,
        scratch_available_bytes: stats
            .f_bavail
            .checked_mul(block_size)
            .ok_or_else(accounting_overflow)?,
        memory_source,
        cpu_source,
    })
}

#[cfg(target_os = "linux")]
fn host_memory_limit() -> Result<(usize, ResourceSource), BifrostResourceError> {
    let contents =
        fs::read_to_string("/proc/meminfo").map_err(|error| BifrostResourceError::Unavailable {
            detail: format!("host memory detection failed: {error}"),
        })?;
    let kib = contents
        .lines()
        .find_map(|line| line.strip_prefix("MemTotal:"))
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or_else(|| BifrostResourceError::Unavailable {
            detail: "host memory total is absent".to_owned(),
        })?;
    let bytes = kib.checked_mul(1024).ok_or_else(accounting_overflow)?;
    Ok((bytes, ResourceSource::Host))
}

#[cfg(not(target_os = "linux"))]
fn host_memory_limit() -> Result<(usize, ResourceSource), BifrostResourceError> {
    Err(BifrostResourceError::Unavailable {
        detail: "host memory probe is unavailable; set WYRD_BIFROST_MEMORY_LIMIT_BYTES".to_owned(),
    })
}

fn read_positive_usize(path: &str) -> Option<usize> {
    let value = fs::read_to_string(path).ok()?;
    let value = value.trim();
    if value == "max" {
        return None;
    }
    value.parse::<usize>().ok().filter(|value| *value > 0)
}

fn detect_cgroup_cpu() -> Option<usize> {
    let quota = fs::read_to_string("/sys/fs/cgroup/cpu.max")
        .ok()
        .and_then(|value| {
            let mut fields = value.split_whitespace();
            let quota = fields.next()?;
            let period = fields.next()?.parse::<usize>().ok()?;
            if quota == "max" || period == 0 {
                return None;
            }
            let quota = quota.parse::<usize>().ok()?;
            Some((quota / period).max(1))
        })
        .or_else(|| {
            let quota = read_positive_usize("/sys/fs/cgroup/cpu/cpu.cfs_quota_us")?;
            let period = read_positive_usize("/sys/fs/cgroup/cpu/cpu.cfs_period_us")?;
            Some((quota / period).max(1))
        });
    let cpuset = fs::read_to_string("/sys/fs/cgroup/cpuset.cpus.effective")
        .ok()
        .or_else(|| fs::read_to_string("/sys/fs/cgroup/cpuset/cpuset.cpus").ok())
        .and_then(|value| parse_cpuset(value.trim()));
    match (quota, cpuset) {
        (Some(quota), Some(cpuset)) => Some(quota.min(cpuset)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn parse_cpuset(value: &str) -> Option<usize> {
    value
        .split(',')
        .try_fold(0usize, |total, part| {
            let mut bounds = part.split('-');
            let start = bounds.next()?.parse::<usize>().ok()?;
            let end = bounds
                .next()
                .map_or(Some(start), |end| end.parse::<usize>().ok())?;
            if end < start || bounds.next().is_some() {
                return None;
            }
            total.checked_add(end - start + 1)
        })
        .filter(|value| *value > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use datafusion::execution::disk_manager::{DiskManagerBuilder, DiskManagerMode};
    use datafusion::execution::memory_pool::MemoryConsumer;
    use datafusion::execution::runtime_env::RuntimeEnvBuilder;
    use datafusion::physical_expr::expressions::Column;
    use datafusion::physical_expr::{LexOrdering, PhysicalSortExpr};
    use datafusion::physical_plan::sorts::sort::SortExec;
    use datafusion::prelude::{SessionConfig, SessionContext};

    /// Every Oracle session owner must set all four Parquet reader pushdown
    /// and indexing options to exactly `true`; a closed leaf predicate only
    /// prunes files, row groups, and pages when the reader is configured to
    /// use it.
    #[test]
    fn oracle_reader_session_options_contract() {
        let config = OracleSessionShape::for_grant(ORACLE_PARTITION_MEMORY_BYTES, 4, 64)
            .apply(datafusion::execution::context::SessionConfig::new());
        let parquet_options = &config.options().execution.parquet;
        assert!(parquet_options.pushdown_filters);
        assert!(parquet_options.reorder_filters);
        assert!(parquet_options.bloom_filter_on_read);
        assert!(parquet_options.enable_page_index);
    }

    fn policy(roles: &[BifrostRole]) -> BifrostResourcePolicy {
        let roles: BTreeSet<BifrostRole> = roles.iter().copied().collect();
        BifrostResourcePolicy {
            roles,
            server_memory_min_bytes: None,
            bifrost_memory_limit_bytes: None,
            scratch_limit_bytes: None,
            effective_cpu: None,
            oracle_query_slot_limit: None,
            scratch_root: PathBuf::new(),
            volume_roots: None,
        }
    }

    /// Builds an injected observation whose default plan has a `cap`-byte
    /// shared Bifrost cap above the default server minimum.
    fn snapshot(cap: usize) -> SystemResourceSnapshot {
        SystemResourceSnapshot {
            memory_limit_bytes: cap + DEFAULT_SERVER_MEMORY_MIN_BYTES,
            effective_cpu: 8,
            scratch_capacity_bytes: 2 * 1024 * 1024 * 1024,
            scratch_available_bytes: 2 * 1024 * 1024 * 1024,
            memory_source: ResourceSource::Injected,
            cpu_source: ResourceSource::Injected,
        }
    }

    /// Boot composition preserves the selected message bound and rejects a clamp.
    ///
    /// # Panics
    ///
    /// Panics when valid composition loses the selected bound or an invalid
    /// message-to-aggregate relationship is silently accepted.
    #[test]
    fn transport_message_limit_is_frozen_separately_from_aggregate_capacity() {
        let selected = 201 * MIB;
        let resources = BifrostRuntimeResources::from_snapshot_with_transport_message_limit(
            snapshot(2 * 1024 * MIB),
            policy(&[BifrostRole::Scribe]),
            selected,
        )
        .expect("selected maximum fits derived aggregate capacity");
        let admission = resources
            .compose_roles()
            .expect("role composition")
            .transport_admission();
        assert_eq!(admission.message_limit_bytes(), selected);
        assert_eq!(
            admission.limit_bytes(),
            resources.plan().managed_memory_bytes
        );

        let aggregate = resources.plan().managed_memory_bytes;
        assert!(
            BifrostRuntimeResources::from_snapshot_with_transport_message_limit(
                snapshot(2 * 1024 * MIB),
                policy(&[BifrostRole::Scribe]),
                aggregate + 1,
            )
            .is_err(),
            "boot must reject rather than clamp a selected maximum above aggregate capacity"
        );
    }

    /// Fills the Oracle budget with workers, leaving room for `spare` more.
    ///
    /// Admission charges slot units rather than a whole memory grant cap, so a
    /// fixture box that once held exactly one worker now holds several. Tests
    /// that need a refusal — races for a last slot, refusal telemetry, floor
    /// protection — must drive the budget to that edge instead of assuming it.
    /// The returned owners must stay alive for the budget to remain full.
    ///
    /// # Panics
    ///
    /// Panics when the governor cannot report a snapshot or refuses a worker
    /// that still fits inside the budget.
    fn saturate_oracle_workers(
        oracle: &OracleResources,
        spare: usize,
    ) -> Vec<OracleWorkerResources> {
        let mut held = Vec::new();
        while let Ok(worker) = oracle.try_acquire_worker(OracleWorkerClass::Interactive) {
            held.push(worker);
        }
        held.truncate(held.len().saturating_sub(spare));
        held
    }

    /// Concurrent Oracle queries compete beneath exactly one governed root.
    ///
    /// Independent per-query pools let three admitted ceilings sum above the
    /// memory the pod owns. This admits an Interactive query, an Analytical
    /// leader, and an Analytical follower whose ceilings do exactly that, then
    /// grows spillable and non-spillable consumers on all three concurrently.
    /// Aggregate fallible growth must stop at the shared root, a refused
    /// reservation must retain nothing, release must restore both the query and
    /// the process baselines without poisoning, and the next query must be
    /// admitted on the same healthy owner.
    ///
    /// # Panics
    ///
    /// Panics when the fixture does not oversubscribe the root, when aggregate
    /// growth exceeds it, when a refusal retains bytes, when a spilling
    /// operator fails untyped, or when teardown leaves the process ledger or
    /// health changed.
    #[tokio::test]
    async fn oracle_queries_share_one_governed_memory_root() {
        // Scratch is deliberately generous: this covers the memory root, and a
        // scratch refusal would stop the fixture before it ever contends.
        let roles = BifrostRuntimeResources::composed_for_test(
            512 * MIB,
            8 * 1024 * MIB as u64,
            [BifrostRole::Oracle],
        );
        let oracle = roles.oracle().expect("Oracle capability");
        let baseline = oracle.snapshot().expect("idle baseline");
        let interactive = oracle
            .try_acquire_query(interactive_query(0.0))
            .expect("interactive leader");
        let analytical = oracle
            .try_acquire_query(analytical_query(0.0))
            .expect("analytical leader");
        let worker = oracle
            .try_acquire_worker(OracleWorkerClass::Analytical)
            .expect("analytical follower");
        let root_limit = oracle.shared_memory_limit();
        assert!(
            interactive.granted_memory_bytes
                + analytical.granted_memory_bytes
                + worker.granted_memory_bytes()
                > root_limit,
            "the fixture must oversubscribe the shared root for this to prove anything"
        );

        let pools = [
            interactive.memory_pool(),
            analytical.memory_pool(),
            worker.memory_pool(),
        ];
        let step = root_limit / 4;
        let admitted = contend_for_shared_root(&pools, step);
        assert!(admitted > 0, "the shared root must admit some growth");
        assert!(
            admitted <= root_limit,
            "aggregate fallible growth must stop at the shared root"
        );
        assert!(
            admitted < pools.len() * 3 * step,
            "the root must have refused growth the independent ceilings would have allowed"
        );

        // A real spilling operator meets the same root. Pinning the whole root
        // through one query leaves the sort no memory, and a query that also
        // has no scratch cannot spill its runs, so it must fail typed rather
        // than exceed the pod.
        let pin = MemoryConsumer::new("shared-root-pin").register(&pools[1]);
        pin.grow(root_limit);
        assert_eq!(oracle.shared_memory_reserved(), root_limit);
        let failure = sort_over_pool(&pools[0])
            .await
            .expect_err("a sort with neither memory nor scratch cannot succeed");
        assert!(
            matches!(failure.find_root(), DataFusionError::ResourcesExhausted(_)),
            "exhaustion must surface as the typed DataFusion resource error, got {failure:?}"
        );
        assert_eq!(pin.free(), root_limit);
        assert_eq!(
            oracle.shared_memory_reserved(),
            0,
            "every released byte returns to the shared root"
        );
        assert_eq!(
            oracle
                .snapshot()
                .expect("post-growth snapshot")
                .oracle_query_memory_used_bytes,
            0,
            "release restores the query baseline"
        );
        assert!(oracle.health().reason().is_none());

        // DataFusion's infallible path cannot refuse, so the ceiling is the only
        // thing that stops one query governing an idle pod's whole root. Grow
        // past this query's grant while the root is empty and the excess must
        // land in explicit headroom, not in governed capacity a sibling query
        // could otherwise have used.
        let ceiling = interactive.granted_memory_bytes;
        let overshoot = ceiling / 2;
        assert!(
            ceiling + overshoot < root_limit,
            "the pod root must still have free bytes for this to prove the ceiling bound"
        );
        let past_ceiling = MemoryConsumer::new("past-ceiling").register(&pools[0]);
        past_ceiling.grow(ceiling + overshoot);
        let past_grant = oracle.snapshot().expect("infallible growth snapshot");
        assert_eq!(
            past_grant.oracle_query_memory_used_bytes, ceiling,
            "governed bytes stop at the query's immutable grant"
        );
        assert_eq!(
            past_grant.infallible_headroom_bytes, overshoot,
            "every byte above the grant is retained as explicit process headroom"
        );
        assert_eq!(oracle.shared_memory_reserved(), ceiling + overshoot);
        assert_eq!(past_ceiling.free(), ceiling + overshoot);
        let released = oracle.snapshot().expect("post-release snapshot");
        assert_eq!(released.oracle_query_memory_used_bytes, 0);
        assert_eq!(released.infallible_headroom_bytes, 0);
        assert_eq!(oracle.shared_memory_reserved(), 0);
        assert!(oracle.health().reason().is_none());

        drop((interactive, analytical, worker));
        assert_eq!(
            oracle.snapshot().expect("released snapshot"),
            baseline,
            "release restores the process baseline exactly"
        );
        drop(
            oracle
                .try_acquire_query(interactive_query(0.0))
                .expect("the next query is admitted on the same healthy owner"),
        );
    }

    /// Grows every supplied query pool concurrently and reports what stuck.
    ///
    /// Each pool registers one consumer, alternating spillable and
    /// non-spillable, and attempts three `step` growths once every thread is
    /// contending. Returns the aggregate bytes the shared root actually
    /// admitted, after every consumer has released again.
    ///
    /// # Panics
    ///
    /// Panics when a refused reservation retains bytes, when release does not
    /// return exactly what was granted, or when a grower thread panics.
    fn contend_for_shared_root(pools: &[Arc<dyn MemoryPool>], step: usize) -> usize {
        // Both barriers are load-bearing: the first makes the three growers
        // contend, the second holds every accepted byte until all of them have
        // finished, so the sum below really is the aggregate peak.
        let barrier = Arc::new(std::sync::Barrier::new(pools.len()));
        std::thread::scope(|scope| {
            let handles: Vec<_> = pools
                .iter()
                .enumerate()
                .map(|(index, pool)| {
                    let barrier = Arc::clone(&barrier);
                    scope.spawn(move || {
                        let reservation = MemoryConsumer::new(format!("shared-root-{index}"))
                            .with_can_spill(index % 2 == 0)
                            .register(pool);
                        barrier.wait();
                        let mut granted = 0;
                        for _ in 0..3 {
                            if reservation.try_grow(step).is_ok() {
                                granted += step;
                            }
                            assert_eq!(
                                reservation.size(),
                                granted,
                                "a refused reservation must retain no bytes"
                            );
                        }
                        barrier.wait();
                        assert_eq!(reservation.free(), granted);
                        assert_eq!(reservation.size(), 0);
                        granted
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().expect("one grower thread"))
                .sum()
        })
    }

    /// Runs one real spilling `SortExec` against the supplied query pool.
    ///
    /// The runtime is deliberately scratch-free: with the disk manager disabled
    /// the sort cannot convert a memory refusal into a spill, so the pool's own
    /// refusal is what the caller observes.
    ///
    /// # Errors
    ///
    /// Returns the `DataFusion` error the sort fails with, which is the typed
    /// resource exhaustion when neither memory nor scratch is available.
    ///
    /// # Panics
    ///
    /// Panics when the fixture batch, ordering, or runtime cannot be built.
    async fn sort_over_pool(pool: &Arc<dyn MemoryPool>) -> Result<(), DataFusionError> {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(Int64Array::from_iter_values((0..8_192_i64).rev()))],
        )
        .expect("the fixture batch matches its own schema");
        let source = datafusion::datasource::memory::MemorySourceConfig::try_new_exec(
            &[vec![batch]],
            schema,
            None,
        )
        .expect("a single-partition memory source accepts one matching batch");
        let ordering = LexOrdering::new(vec![PhysicalSortExpr::new(
            Arc::new(Column::new("value", 0)),
            arrow::compute::SortOptions::default(),
        )])
        .expect("a one-column ordering is non-empty");
        let runtime = RuntimeEnvBuilder::new()
            .with_memory_pool(Arc::clone(pool))
            .with_disk_manager_builder(
                DiskManagerBuilder::default().with_mode(DiskManagerMode::Disabled),
            )
            .build_arc()
            .expect("a bounded scratch-free runtime is constructible");
        let context = SessionContext::new_with_config_rt(SessionConfig::new(), runtime).task_ctx();
        datafusion::physical_plan::collect(Arc::new(SortExec::new(ordering, source)), context)
            .await
            .map(|_| ())
    }

    /// Builds one exact interactive query quantum for root-ledger tests.
    fn interactive_query(local_ratio: f64) -> OracleResourceRequest {
        OracleResourceRequest::for_class(QueryClass::Interactive, local_ratio)
    }

    /// Builds one exact analytical query quantum for root-ledger tests.
    fn analytical_query(local_ratio: f64) -> OracleResourceRequest {
        OracleResourceRequest::for_class(QueryClass::Analytical, local_ratio)
    }

    /// One process governor holds one shared cap that every role and transport
    /// competes for, with no idle-role partition and no precharge.
    ///
    /// An 8-GiB observation yields 1-GiB server headroom and a 7-GiB cap; a
    /// raised minimum or a lower operator cap narrows it; impossible settings
    /// refuse. Scribe, Oracle, Forge, and transport then charge concurrently
    /// until the cap refuses one more byte, the refusal retains nothing, and
    /// release re-admits the same charge.
    ///
    /// # Panics
    ///
    /// Panics when a plan resolves differently, a charge is refused below the
    /// cap, a refusal retains bytes, or release does not restore the baseline.
    #[test]
    fn shared_cap_defaults_overrides_and_concurrent_charges() {
        let gib = 1024 * MIB;
        let all = [BifrostRole::Scribe, BifrostRole::Oracle, BifrostRole::Forge];
        let mut eight = snapshot(0);
        eight.memory_limit_bytes = 8 * gib;
        let plan = |server: Option<usize>, cap: Option<usize>| {
            let mut policy = policy(&all);
            policy.server_memory_min_bytes = server;
            policy.bifrost_memory_limit_bytes = cap;
            BifrostRuntimeResources::from_snapshot(eight, policy).map(|runtime| runtime.plan())
        };
        let default = plan(None, None).expect("default 8-GiB plan");
        assert_eq!(default.server_memory_min_bytes, gib);
        assert_eq!(default.managed_memory_bytes, 7 * gib);
        assert_eq!(
            plan(Some(2 * gib), None)
                .expect("raised minimum")
                .managed_memory_bytes,
            6 * gib
        );
        assert_eq!(
            plan(None, Some(4 * gib))
                .expect("lower cap")
                .managed_memory_bytes,
            4 * gib
        );
        for (server, cap) in [
            (Some(gib - 1), None),
            (Some(8 * gib), None),
            (None, Some(0)),
            (None, Some(7 * gib + 1)),
        ] {
            assert!(
                matches!(
                    plan(server, cap),
                    Err(BifrostResourceError::InvalidPlan { .. })
                ),
                "server {server:?} cap {cap:?} must refuse boot"
            );
        }

        let roles = BifrostRuntimeResources::composed_for_test(64 * MIB, 512 * MIB as u64, all);
        let idle = roles.snapshot().expect("idle snapshot");
        assert_eq!(
            idle.governed_memory_used_bytes, 0,
            "idle roles hold nothing"
        );
        let scribe = roles.scribe().expect("Scribe capability");
        let oracle = roles.oracle().expect("Oracle capability");
        let forge = roles.forge().expect("Forge capability");
        let transport = roles.transport_admission();
        let query = oracle
            .try_acquire_query(interactive_query(0.0))
            .expect("Oracle admission charges no memory");
        let quarter = 16 * MIB;
        let (scribe_owner, oracle_bytes, forge_bytes, transport_lease) =
            std::thread::scope(|scope| {
                let scribe_owner = scope.spawn(|| {
                    scribe
                        .try_acquire_memory(ScribeMemoryRequest {
                            bytes: quarter,
                            category: ScribeMemoryCategory::Raw,
                            shard: Some(0),
                        })
                        .expect("Scribe charge")
                });
                let oracle_pool = query.memory_pool();
                let oracle_bytes = scope.spawn(move || {
                    let consumer = MemoryConsumer::new("shared-cap-oracle").register(&oracle_pool);
                    consumer.try_grow(quarter).expect("Oracle charge");
                    consumer
                });
                let forge_pool = forge.rewrite_memory_pool();
                let forge_bytes = scope.spawn(move || {
                    let consumer = MemoryConsumer::new("shared-cap-forge").register(&forge_pool);
                    consumer.try_grow(quarter).expect("Forge charge");
                    consumer
                });
                let transport_lease =
                    scope.spawn(|| transport.try_acquire(quarter).expect("transport charge"));
                (
                    scribe_owner.join().expect("Scribe thread"),
                    oracle_bytes.join().expect("Oracle thread"),
                    forge_bytes.join().expect("Forge thread"),
                    transport_lease.join().expect("transport thread"),
                )
            });
        let full = roles.snapshot().expect("full snapshot");
        assert_eq!(full.governed_memory_used_bytes, 64 * MIB);
        for held in [
            full.scribe_memory_used_bytes,
            full.oracle_memory_used_bytes,
            full.forge_memory_used_bytes,
            full.transport_memory_used_bytes,
        ] {
            assert_eq!(held, quarter, "role attribution is diagnostic only");
        }
        assert!(
            transport.try_acquire(1).is_err(),
            "the cap refuses one more byte"
        );
        assert!(
            MemoryConsumer::new("refused")
                .register(&forge.rewrite_memory_pool())
                .try_grow(1)
                .is_err()
        );
        assert_eq!(
            roles.snapshot().expect("refused snapshot"),
            full,
            "a refused charge retains nothing"
        );
        drop((scribe_owner, oracle_bytes, forge_bytes));
        assert_eq!(
            roles
                .snapshot()
                .expect("partial release")
                .governed_memory_used_bytes,
            quarter
        );
        let readmitted = scribe
            .try_acquire_memory(ScribeMemoryRequest {
                bytes: 3 * quarter,
                category: ScribeMemoryCategory::Raw,
                shard: Some(0),
            })
            .expect("released bytes are re-admitted to any holder");
        drop((readmitted, transport_lease, query));
        assert_eq!(roles.snapshot().expect("released snapshot"), idle);
        assert!(roles.health().reason().is_none());
    }

    /// Injected runtime construction uses one policy stage and one root owner.
    #[test]
    fn runtime_resources_live_and_injected_paths_share_one_policy_stage() {
        let policy = policy(&[BifrostRole::Scribe, BifrostRole::Oracle]);
        let runtime = BifrostRuntimeResources::from_snapshot(snapshot(1024 * MIB), policy)
            .expect("complete injected observation must construct");
        let roles = runtime
            .compose_roles()
            .expect("composition must be issued from an unpoisoned root");
        assert!(runtime.shares_root_with(&roles));
        assert_eq!(runtime.plan(), roles.plan());
        assert_eq!(
            roles.plan().managed_memory_bytes,
            1024 * MIB,
            "the role capability projects the sole root plan"
        );
        assert!(roles.oracle().is_some());
        assert_eq!(runtime.sources().memory, ResourceSource::Injected);
    }

    /// Invalid complete observations fail before any role handle is returned.
    #[test]
    fn runtime_resources_reject_invalid_snapshot_before_role_activation() {
        let memory_error =
            BifrostRuntimeResources::from_snapshot(snapshot(0), policy(&[BifrostRole::Oracle]));
        assert!(matches!(
            memory_error,
            Err(BifrostResourceError::InvalidPlan { .. })
        ));

        let mut no_scratch = snapshot(768 * MIB);
        no_scratch.scratch_available_bytes = MIN_SCRATCH_FREE_BYTES;
        let scratch_error =
            BifrostRuntimeResources::from_snapshot(no_scratch, policy(&[BifrostRole::Oracle]));
        assert!(matches!(
            scratch_error,
            Err(BifrostResourceError::InvalidPlan { .. })
        ));
    }

    /// Scribe in mixed topology admits a generation and producer delta up to
    /// the whole shared cap, because idle Oracle and Forge roles hold nothing.
    ///
    /// This models the production handoff: the immutable generation remains
    /// charged while the Parquet producer acquires only the complement to the
    /// cap. Together they consume, but never exceed, the shared cap, and
    /// dropping both owners returns root attribution to baseline.
    ///
    /// # Panics
    ///
    /// Panics if exact-floor composition, ingress admission, category transfer,
    /// producer admission, attribution inspection, or release reconciliation
    /// violates the Scribe ownership contract.
    #[test]
    fn scribe_shared_cap_admits_generation_and_transfer() {
        let roles = BifrostRuntimeResources::composed_for_test(
            832 * MIB,
            512 * MIB as u64,
            [BifrostRole::Scribe, BifrostRole::Oracle, BifrostRole::Forge],
        );
        let scribe = roles.scribe().expect("Scribe capability");
        let cap = scribe.governor.plan().managed_memory_bytes;
        assert_eq!(cap, 832 * MIB);
        assert_eq!(scribe.ingress_limit_bytes(), cap);

        // Derive the generation from the projection rather than hard-coding
        // both sides, so a change to the workspace formula keeps the scenario
        // exact instead of silently overshooting the cap.
        let producer_delta = crate::scribe::memory::parquet_candidate_incremental_bytes(72 * MIB)
            .expect("candidate workspace projection");
        let generation_bytes = cap - producer_delta;
        let mut generation = scribe
            .try_reserve_ingress(ScribeMemoryCategory::Active, generation_bytes)
            .expect("representative generation must fit the shared cap");
        generation
            .transfer_category(ScribeMemoryCategory::Immutable)
            .expect("generation ownership must transfer to immutable");
        let producer = scribe
            .try_reserve_maintenance(ScribeMemoryCategory::Persistence, producer_delta)
            .expect("producer delta must complete the shared cap");
        // Deriving the generation makes the sum equal the cap by construction,
        // so assert the consequence that is not tautological: the cap is
        // genuinely full and one further byte is refused.
        assert!(
            matches!(
                scribe.try_reserve_ingress(ScribeMemoryCategory::Active, 1),
                Err(crate::contracts::ScribeError::IngestBusy { .. })
            ),
            "generation plus producer workspace must exactly exhaust the shared cap"
        );

        let occupied = scribe.snapshot().expect("occupied Scribe snapshot");
        assert_eq!(
            occupied.scribe_memory_used_bytes,
            generation_bytes + producer_delta
        );
        assert_eq!(occupied.governed_memory_used_bytes, cap);
        let attribution = scribe
            .governor
            .attribution_snapshot()
            .expect("producer handoff attribution");
        assert_eq!(
            attribution.category_bytes[ScribeMemoryCategory::Immutable as usize],
            generation_bytes
        );
        assert_eq!(
            attribution.category_bytes[ScribeMemoryCategory::Persistence as usize],
            producer_delta
        );

        drop(producer);
        drop(generation);
        assert_eq!(
            scribe
                .snapshot()
                .expect("released Scribe snapshot")
                .scribe_memory_used_bytes,
            0
        );
    }

    /// Mixed-topology ingress resize is bounded by the shared cap alone.
    ///
    /// # Panics
    ///
    /// Panics if a cap-sized resize is refused, a byte beyond the cap is
    /// admitted, or releasing the resized owner fails to restore baseline.
    #[test]
    fn scribe_ingress_resize_is_bounded_by_shared_cap() {
        let roles = BifrostRuntimeResources::composed_for_test(
            832 * MIB,
            512 * MIB as u64,
            [BifrostRole::Scribe, BifrostRole::Oracle, BifrostRole::Forge],
        );
        let scribe = roles.scribe().expect("Scribe capability");
        let mut ingress = scribe
            .try_reserve_ingress(ScribeMemoryCategory::Raw, 1)
            .expect("initial ingress owner");

        ingress
            .resize_ingress(832 * MIB)
            .expect("cap-sized resize must remain admissible");
        assert!(ingress.resize_ingress(832 * MIB + 1).is_err());
        assert_eq!(ingress.bytes(), 832 * MIB);

        drop(ingress);
        assert_eq!(
            scribe
                .snapshot()
                .expect("released ingress snapshot")
                .scribe_memory_used_bytes,
            0
        );
    }

    /// Concurrent ingress requests enforce their shared sublimit in one root transition.
    ///
    /// # Panics
    ///
    /// Panics when the production-equivalent composition, thread coordination,
    /// root admission, or authoritative snapshot violates the ingress ceiling.
    #[test]
    fn scribe_ingress_sublimit_is_atomic_under_concurrency() {
        let roles = BifrostRuntimeResources::from_snapshot(
            snapshot(1024 * MIB),
            policy(&[BifrostRole::Scribe]),
        )
        .expect("Scribe runtime resources")
        .compose_roles()
        .expect("Scribe role composition");
        let scribe = roles.scribe().expect("Scribe capability");
        let ingress_limit = scribe.ingress_limit_bytes();
        let request_bytes = ingress_limit / 2 + 1;
        let start = Arc::new(std::sync::Barrier::new(3));
        let finish = Arc::new(std::sync::Barrier::new(3));
        let (sender, receiver) = std::sync::mpsc::channel();
        let mut joins = Vec::new();
        for _ in 0..2 {
            let scribe = scribe.clone();
            let start = Arc::clone(&start);
            let finish = Arc::clone(&finish);
            let sender = sender.clone();
            joins.push(std::thread::spawn(move || {
                start.wait();
                let owner = scribe.try_reserve_ingress(ScribeMemoryCategory::Raw, request_bytes);
                sender.send(owner.is_ok()).expect("ingress race result");
                finish.wait();
                drop(owner);
            }));
        }
        start.wait();
        let admitted = [
            receiver.recv().expect("first ingress result"),
            receiver.recv().expect("second ingress result"),
        ];
        assert_eq!(admitted.into_iter().filter(|value| *value).count(), 1);
        assert!(
            scribe
                .snapshot()
                .expect("authoritative ingress snapshot")
                .scribe_memory_used_bytes
                <= ingress_limit
        );
        finish.wait();
        for join in joins {
            join.join().expect("ingress race thread");
        }
        assert_eq!(
            scribe
                .snapshot()
                .expect("released ingress snapshot")
                .scribe_memory_used_bytes,
            0
        );
    }

    /// Concurrent Oracle owners atomically admit the last slot unit without rollback drift.
    ///
    /// # Panics
    ///
    /// Panics when thread coordination or the saturated fixture fails.
    #[test]
    fn resource_plan_concurrent_oracle_acquisition_is_atomic() {
        let roles = BifrostRuntimeResources::composed_for_test(
            576 * MIB,
            512 * MIB as u64,
            [BifrostRole::Oracle, BifrostRole::Forge],
        );
        let oracle = roles.oracle().expect("Oracle capability");
        let filled = saturate_oracle_workers(&oracle, 1);
        let charged_before = oracle
            .snapshot()
            .expect("saturated to one spare slot")
            .oracle_query_slot_units;
        let start = Arc::new(std::sync::Barrier::new(3));
        let finish = Arc::new(std::sync::Barrier::new(3));
        let (sender, receiver) = std::sync::mpsc::channel();
        let mut joins = Vec::new();
        for _ in 0..2 {
            let oracle = oracle.clone();
            let start = Arc::clone(&start);
            let finish = Arc::clone(&finish);
            let sender = sender.clone();
            joins.push(std::thread::spawn(move || {
                start.wait();
                let owner = oracle.try_acquire_worker(OracleWorkerClass::Interactive);
                sender.send(owner.is_ok()).expect("race result");
                finish.wait();
                drop(owner);
            }));
        }
        start.wait();
        let admitted = [
            receiver.recv().expect("first result"),
            receiver.recv().expect("second result"),
        ];
        assert_eq!(admitted.into_iter().filter(|value| *value).count(), 1);
        assert_eq!(
            oracle
                .snapshot()
                .expect("held owner")
                .oracle_query_slot_units,
            charged_before + 1,
            "exactly one racer charged the last remaining slot unit"
        );
        finish.wait();
        for join in joins {
            join.join().expect("Oracle race thread");
        }
        drop(filled);
        assert_eq!(
            oracle
                .snapshot()
                .expect("released owners")
                .oracle_memory_used_bytes,
            0
        );
    }

    /// Registration clears only process-owned claim directories, and a dropped
    /// claim leaves its directory for the next registration.
    ///
    /// # Panics
    ///
    /// Panics when deterministic namespace setup or cleanup fails.
    #[test]
    fn scribe_output_scratch_namespace_is_exactly_scoped() {
        let temp = tempfile::tempdir().expect("temporary volume root");
        let wal = temp.path().join("wal");
        let stage_root = wal.join("scribe-stage");
        let scribe = wal.join("scribe-output-scratch");
        for path in [&wal, &stage_root, &scribe] {
            fs::create_dir_all(path).expect("registered volume root");
        }
        let retained_wal = wal.join("retained.wal");
        let peer = scribe.join("peer-owned");
        let stale = scribe.join("scribe-claim-old-claim-0");
        fs::write(&retained_wal, [1_u8]).expect("retained WAL");
        fs::create_dir(&peer).expect("peer directory");
        fs::create_dir(&stale).expect("stale runtime directory");

        let output = ScratchVolume::register(scribe.clone())
            .expect("registration reconciles process-owned scratch");
        assert!(!stale.exists());
        assert!(peer.exists());
        assert!(retained_wal.exists());

        let scratch = output
            .create_scribe_claim("stream_1", "claim_9")
            .expect("claim scratch");
        let owned = scratch.path().to_owned();
        assert_eq!(owned.parent(), Some(scribe.as_path()));
        assert!(
            owned
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("scribe-claim-stream_1-claim_9-"))
        );
        drop(scratch);
        assert!(owned.exists(), "a dropped claim leaves its directory");
        ScratchVolume::register(scribe).expect("restart reconciles the dropped claim");
        assert!(!owned.exists());
        assert!(peer.exists());
        assert!(retained_wal.exists());
    }

    /// Scratch cleanup attempts immediately and then after every configured delay.
    ///
    /// # Panics
    ///
    /// Panics when first-attempt success retries, or when any transient failure
    /// count does not consume exactly that many retries before succeeding.
    #[test]
    fn scratch_cleanup_retry_attempts_have_exact_semantics() {
        let immediate_attempts = std::cell::Cell::new(0);
        retry_scratch_cleanup(|| {
            immediate_attempts.set(immediate_attempts.get() + 1);
            Ok(())
        })
        .expect("first cleanup attempt succeeds");
        assert_eq!(immediate_attempts.get(), 1);

        for transient_failures in 1..=SCRATCH_CLEANUP_BACKOFFS.len() {
            let attempts = std::cell::Cell::new(0);
            retry_scratch_cleanup(|| {
                attempts.set(attempts.get() + 1);
                if attempts.get() <= transient_failures {
                    Err(std::io::Error::other("transient cleanup failure"))
                } else {
                    Ok(())
                }
            })
            .expect("cleanup succeeds after the configured transient failure");
            assert_eq!(attempts.get(), transient_failures + 1);
        }
    }

    /// Closed resource telemetry covers plans, grants, refusals, and releases.
    ///
    /// # Panics
    ///
    /// Panics when the deterministic owner lifecycle or metric assertions fail.
    #[test]
    fn resource_plan_metrics_cover_closed_memory_lifecycle() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        metrics::with_local_recorder(&recorder, || {
            let roles = BifrostRuntimeResources::composed_for_test(
                576 * MIB,
                512 * MIB as u64,
                [BifrostRole::Oracle, BifrostRole::Forge],
            );
            let oracle = roles.oracle().expect("Oracle capability");
            // Followers hold slot units rather than root memory now, so the
            // fixed metadata slot is the owner whose refusal the memory metrics
            // report.
            let mut filled = Vec::new();
            while let Ok(slot) = oracle.metadata().try_acquire_footer_slot() {
                filled.push(slot);
            }
            assert!(
                !filled.is_empty(),
                "the Oracle budget admits at least one fixed slot"
            );
            assert!(
                oracle.metadata().try_acquire_footer_slot().is_err(),
                "a saturated budget refuses and records the refusal"
            );
            drop(filled);
        });
        let snapshot = recorder.snapshot();
        let metric_exists = |fragment: &str| {
            snapshot
                .counters
                .keys()
                .chain(snapshot.gauges.keys())
                .any(|name| name.contains(fragment))
        };
        for fragment in [
            "bifrost_resource_planned_bytes",
            "role=\"oracle\"",
            "result=\"acquired\"",
            "result=\"refused\"",
            "result=\"released\"",
            "bifrost_resource_current_bytes",
        ] {
            assert!(
                metric_exists(fragment),
                "missing metric fragment {fragment}"
            );
        }
    }

    /// Slot and scratch ownership is one atomic Oracle grant and exact release.
    #[test]
    fn resource_grant_is_atomic_across_memory_and_scratch() {
        let roles = BifrostRuntimeResources::composed_for_test(
            768 * MIB,
            512 * MIB as u64,
            [BifrostRole::Scribe, BifrostRole::Oracle],
        );
        let oracle = roles.oracle().expect("Oracle capability");
        let first = oracle
            .try_acquire_query(interactive_query(0.0))
            .expect("first query owns one exact grant");
        // Scratch, not memory, is the dimension that saturates this fixture:
        // admission debits no resident memory at all, so each query takes slot
        // units plus a full grant cap of consumable disk.
        let mut held = Vec::new();
        while let Ok(query) = oracle.try_acquire_query(interactive_query(1.0)) {
            held.push(query);
        }
        assert!(
            oracle.try_acquire_query(interactive_query(1.0)).is_err(),
            "a saturated dimension refuses further exact grants"
        );
        let occupied = oracle.snapshot().expect("snapshot");
        assert!(occupied.oracle_query_active);
        drop(held);
        drop(first);
        assert_eq!(
            oracle.snapshot().expect("released snapshot"),
            ResourceSnapshot {
                plan: roles.plan(),
                governed_memory_used_bytes: 0,
                scribe_memory_used_bytes: 0,
                oracle_memory_used_bytes: 0,
                forge_memory_used_bytes: 0,
                transport_memory_used_bytes: 0,
                oracle_active_queries: 0,
                oracle_interactive_queries: 0,
                oracle_analytical_queries: 0,
                oracle_query_slot_units: 0,
                oracle_query_memory_used_bytes: 0,
                infallible_headroom_bytes: 0,
                oracle_query_active: false,
            }
        );
    }

    /// Adaptive partitions reproduce the locked locality and memory tuples.
    #[test]
    fn oracle_partition_count_clamps_locality_by_memory() {
        let gib = 1024 * MIB;
        assert_eq!(
            oracle_target_partitions(8, 0.0, 8 * gib).expect("remote"),
            32
        );
        assert_eq!(
            oracle_target_partitions(8, 0.5, 8 * gib).expect("mixed"),
            20
        );
        assert_eq!(oracle_target_partitions(8, 1.0, 8 * gib).expect("local"), 8);
        assert!(oracle_target_partitions(usize::MAX, 0.0, gib).is_err());
    }

    /// An Interactive envelope keeps CPU parallelism instead of collapsing to one.
    ///
    /// The Interactive class is granted exactly one
    /// [`ORACLE_PARTITION_MEMORY_BYTES`] envelope. Clamping partitions by that
    /// same quantum yielded one serial partition for every interactive query
    /// regardless of core count; the working-memory quantum preserves CPU-driven
    /// parallelism while still bounding memory per partition.
    #[test]
    fn interactive_envelope_keeps_cpu_parallelism() {
        let interactive = oracle_target_partitions(8, 0.0, ORACLE_PARTITION_MEMORY_BYTES)
            .expect("interactive envelope admits partitions");
        assert_eq!(
            interactive,
            ORACLE_PARTITION_MEMORY_BYTES / ORACLE_PARTITION_WORKING_MEMORY_BYTES,
            "interactive partitions must be bounded by working memory, not the envelope quantum"
        );
        assert!(
            interactive > 1,
            "an interactive query must never execute on a single serial partition"
        );
    }

    /// Every admitted query keeps the minimum partition floor.
    #[test]
    fn partition_ceiling_never_falls_below_the_floor() {
        assert_eq!(
            oracle_target_partitions(1, 1.0, ORACLE_PARTITION_WORKING_MEMORY_BYTES)
                .expect("single-core local scan"),
            ORACLE_MIN_TARGET_PARTITIONS
        );
    }

    /// Work narrowing caps parallelism at the scannable units and holds the floor.
    #[test]
    fn work_narrowing_caps_partitions_without_breaking_the_floor() {
        assert_eq!(
            oracle_partitions_for_work(32, 5),
            5,
            "a five-unit cut must not fan out to the full admitted ceiling"
        );
        assert_eq!(
            oracle_partitions_for_work(32, 64),
            32,
            "work beyond the admitted ceiling cannot raise parallelism"
        );
        assert_eq!(
            oracle_partitions_for_work(32, 1),
            ORACLE_MIN_TARGET_PARTITIONS,
            "a single-unit cut still keeps the minimum partition floor"
        );
        assert_eq!(
            oracle_partitions_for_work(32, 0),
            ORACLE_MIN_TARGET_PARTITIONS,
            "an empty cut still executes at the minimum partition floor"
        );
    }

    /// Portable source precedence selects the tightest injected host/cgroup bounds.
    #[test]
    fn resource_detector_resolves_portable_sources_in_order() {
        let mut injected = snapshot(2 * 1024 * MIB);
        injected.memory_limit_bytes = 1024 * MIB + DEFAULT_SERVER_MEMORY_MIN_BYTES;
        injected.memory_source = ResourceSource::CgroupV2;
        injected.effective_cpu = 3;
        injected.cpu_source = ResourceSource::CgroupV1;
        let runtime =
            BifrostRuntimeResources::from_snapshot(injected, policy(&[BifrostRole::Oracle]))
                .expect("injected portable sources must produce a plan");
        assert_eq!(runtime.sources().memory, ResourceSource::CgroupV2);
        assert_eq!(runtime.sources().cpu, ResourceSource::CgroupV1);
        assert_eq!(runtime.sources().scratch, ResourceSource::Filesystem);
        assert_eq!(parse_cpuset("0-2,5"), Some(4));
        assert_eq!(parse_cpuset("4-2"), None);
    }

    /// Absolute overrides can tighten but never inflate detected resources.
    #[test]
    fn resource_detector_uses_tightest_host_cgroup_and_override_bound() {
        let mut policy = policy(&[BifrostRole::Oracle]);
        policy.bifrost_memory_limit_bytes = Some(768 * MIB);
        policy.effective_cpu = Some(2);
        let runtime = BifrostRuntimeResources::from_snapshot(snapshot(1024 * MIB), policy)
            .expect("reducing overrides must be accepted");
        assert_eq!(
            runtime.plan().memory_limit_bytes,
            1024 * MIB + DEFAULT_SERVER_MEMORY_MIN_BYTES,
            "the process limit stays the observation"
        );
        assert_eq!(runtime.plan().managed_memory_bytes, 768 * MIB);
        assert_eq!(runtime.plan().effective_cpu, 2);
        assert_eq!(runtime.sources().cpu, ResourceSource::Override);
    }

    /// Overrides cap one global plan and never create per-role silos.
    #[test]
    fn resource_detector_applies_absolute_overrides_without_role_silos() {
        let mut policy = policy(&[BifrostRole::Scribe, BifrostRole::Oracle]);
        policy.bifrost_memory_limit_bytes = Some(512 * MIB);
        policy.scratch_limit_bytes = Some(512 * MIB as u64);
        let runtime = BifrostRuntimeResources::from_snapshot(snapshot(1024 * MIB), policy)
            .expect("combined minimum must fit");
        assert_eq!(runtime.plan().managed_memory_bytes, 512 * MIB);
        assert_eq!(runtime.plan().scratch_limit_bytes, 512 * MIB as u64);
        assert_eq!(runtime.sources().scratch, ResourceSource::Override);
    }

    /// Minimum process and filesystem reserves fail closed before activation.
    #[test]
    fn resource_plan_enforces_minimum_viable_process_and_disk_floors() {
        assert!(
            BifrostRuntimeResources::from_snapshot(snapshot(0), policy(&[BifrostRole::Oracle]),)
                .is_err()
        );
        let mut insufficient_disk = snapshot(768 * MIB);
        insufficient_disk.scratch_available_bytes = MIN_SCRATCH_FREE_BYTES;
        assert!(
            BifrostRuntimeResources::from_snapshot(
                insufficient_disk,
                policy(&[BifrostRole::Oracle]),
            )
            .is_err()
        );
    }

    /// One exact Oracle grant owns one finite greedy pool and releases exactly.
    #[test]
    fn oracle_runtime_pool_is_issued_by_query_lease() {
        let roles = BifrostRuntimeResources::from_snapshot(
            snapshot(1024 * MIB),
            policy(&[BifrostRole::Scribe, BifrostRole::Oracle]),
        )
        .expect("combined plan")
        .compose_roles()
        .expect("combined role composition");
        let oracle = roles.oracle().expect("Oracle capability");
        let query = oracle
            .try_acquire_query(interactive_query(0.0))
            .expect("complete query grant");
        assert_eq!(
            oracle
                .snapshot()
                .expect("admitted snapshot")
                .oracle_query_memory_used_bytes,
            0,
            "admission grants a ceiling and charges no memory of its own"
        );
        let pool = query.memory_pool();
        let reservation = MemoryConsumer::new("oracle-test").register(&pool);
        reservation
            .try_grow(query.granted_memory_bytes)
            .expect("the view is sized by the grant this query was admitted with");
        assert!(reservation.try_grow(1).is_err());
        reservation.shrink(query.granted_memory_bytes);
        assert_eq!(pool.reserved(), 0);
        drop(query);
        assert!(!roles.snapshot().expect("snapshot").oracle_query_active);
    }

    /// Only a slot or scratch return advances the epoch Oracle admission waits on.
    ///
    /// A queued query can be refused only for slots or scratch, so resident
    /// query-memory churn must leave the epoch alone: every advance wakes the
    /// whole admission queue to re-run a grant pass that cannot succeed.
    #[tokio::test]
    async fn oracle_capacity_epoch_ignores_query_memory_and_advances_on_release() {
        let roles = BifrostRuntimeResources::from_snapshot(
            snapshot(1024 * MIB),
            policy(&[BifrostRole::Scribe, BifrostRole::Oracle]),
        )
        .expect("combined plan")
        .compose_roles()
        .expect("combined role composition");
        let oracle = roles.oracle().expect("Oracle capability");
        let query = oracle
            .try_acquire_query(interactive_query(0.0))
            .expect("complete query grant");
        let observed = oracle.capacity_epoch();
        let reservation = MemoryConsumer::new("oracle-epoch").register(&query.memory_pool());
        reservation.try_grow(MIB).expect("query memory growth");
        reservation.shrink(MIB);
        assert_eq!(
            oracle.capacity_epoch(),
            observed,
            "query-memory churn is not an admission capacity change"
        );
        drop(reservation);
        drop(query);
        let advanced = oracle
            .wait_for_capacity_change(observed)
            .await
            .expect("a query release advances the epoch");
        assert!(advanced > observed);
    }

    /// Live detection reaches the same checked constructor as an injection.
    ///
    /// `detect` may legitimately fail on a constrained CI host. Scratch free
    /// space is sampled independently and may change between observations, so
    /// this compares every deterministic plan field while validating both
    /// scratch results through the shared checked constructor.
    #[test]
    fn live_detection_delegates_to_checked_snapshot_construction() {
        let root = tempfile::tempdir().expect("scratch root");
        let mut policy = policy(&[BifrostRole::Oracle]);
        policy.scratch_root = root.path().to_owned();
        policy.bifrost_memory_limit_bytes = Some(512 * MIB);
        match BifrostRuntimeResources::detect(policy.clone()) {
            Ok(runtime) => {
                assert_eq!(runtime.plan().managed_memory_bytes, 512 * MIB);
                assert_eq!(runtime.sources().scratch, ResourceSource::Filesystem);
                let detected = detect_snapshot(&policy.scratch_root, None)
                    .expect("detection succeeded once already");
                let replayed = BifrostRuntimeResources::from_snapshot(detected, policy)
                    .expect("the injected path accepts the detected observation");
                let live_plan = runtime.plan();
                let replayed_plan = replayed.plan();
                assert_eq!(
                    (
                        replayed_plan.memory_limit_bytes,
                        replayed_plan.effective_cpu,
                        replayed_plan.server_memory_min_bytes,
                        replayed_plan.managed_memory_bytes,
                    ),
                    (
                        live_plan.memory_limit_bytes,
                        live_plan.effective_cpu,
                        live_plan.server_memory_min_bytes,
                        live_plan.managed_memory_bytes,
                    ),
                    "detection must resolve deterministic fields through one constructor"
                );
                assert!(live_plan.scratch_limit_bytes > 0);
                assert!(replayed_plan.scratch_limit_bytes > 0);
            }
            Err(error) => assert!(
                matches!(
                    error,
                    BifrostResourceError::Unavailable { .. }
                        | BifrostResourceError::InvalidPlan { .. }
                ),
                "detection may only fail through the shared checked stage: {error}"
            ),
        }
    }

    /// One composition issues every capability from the single retained root.
    #[test]
    fn role_composition_issues_capabilities_from_one_root() {
        let runtime = BifrostRuntimeResources::from_snapshot(
            snapshot(2 * 1024 * MIB),
            policy(&[BifrostRole::Scribe, BifrostRole::Oracle, BifrostRole::Forge]),
        )
        .expect("all-role plan must fit");
        let roles = runtime.compose_roles().expect("composition");
        let oracle = roles.oracle().expect("Oracle capability");
        assert!(oracle.shares_root_with(&roles));
        assert!(runtime.shares_root_with(&roles));

        let request = interactive_query(0.0);
        let lease = oracle
            .try_acquire_query(request)
            .expect("Oracle query lease");
        let occupied = roles.snapshot().expect("the root observes its own lease");
        // Admission itself reserves no memory: the class quantum is a ceiling
        // and a partition-planning input, and the root is charged only as this
        // query's `DataFusion` consumers actually grow.
        assert_eq!(occupied.oracle_memory_used_bytes, 0);
        assert_eq!(occupied.oracle_query_slot_units, request.slot_units);
        assert!(lease.granted_memory_bytes >= request.memory_bytes);
        let child = lease
            .try_split_memory("root-composition-test", MIB)
            .expect("one named child of the shared root");
        assert_eq!(
            roles
                .snapshot()
                .expect("the root observes a live child")
                .oracle_memory_used_bytes,
            MIB,
            "a query child competes beneath the shared process root"
        );
        drop(child);
        drop(lease);
        assert_eq!(
            oracle
                .snapshot()
                .expect("released")
                .governed_memory_used_bytes,
            0
        );
    }

    /// Scribe ownership transformations preserve one root and exact attribution.
    #[test]
    fn material_upper_bound_owns_single_materialization_and_exact_shrink_is_atomic() {
        let roles = BifrostRuntimeResources::composed_for_test(
            768 * MIB,
            512 * MIB as u64,
            [BifrostRole::Scribe, BifrostRole::Oracle],
        );
        let scribe = roles.scribe().expect("Scribe capability");
        let mut owner = scribe
            .try_acquire_memory(ScribeMemoryRequest {
                bytes: 96 * MIB,
                category: crate::scribe::memory::MemoryCategory::Active,
                shard: Some(3),
            })
            .expect("root admission");
        let child = owner.split(32 * MIB).expect("checked split");
        owner.merge(child).expect("checked merge");
        owner.resize(128 * MIB).expect("checked growth");
        owner
            .reclassify(crate::scribe::memory::MemoryCategory::Immutable)
            .expect("net-zero reclassification");
        let attribution = scribe
            .governor
            .attribution_snapshot()
            .expect("reconciled attribution");
        assert_eq!(
            attribution.category_bytes[crate::scribe::memory::MemoryCategory::Immutable as usize],
            128 * MIB
        );
        assert_eq!(attribution.shard_bytes.get(&3), Some(&(128 * MIB)));
        let before_shrink = scribe.snapshot().expect("pre-shrink snapshot");
        owner.shrink_to(64 * MIB).expect("atomic exact shrink");
        let after_shrink = scribe.snapshot().expect("post-shrink snapshot");
        assert_eq!(
            before_shrink.scribe_memory_used_bytes - after_shrink.scribe_memory_used_bytes,
            64 * MIB
        );
        assert_eq!(owner.bytes(), 64 * MIB);
        drop(owner);
        assert_eq!(
            scribe
                .governor
                .snapshot()
                .expect("released snapshot")
                .scribe_memory_used_bytes,
            0
        );
    }

    /// Epoch waiting observes a release that occurs before waiter registration.
    #[tokio::test]
    async fn accepted_replay_capacity_wait_is_bounded_and_cancellation_safe() {
        let roles = BifrostRuntimeResources::composed_for_test(
            768 * MIB,
            512 * MIB as u64,
            [BifrostRole::Scribe, BifrostRole::Oracle],
        );
        let scribe = roles.scribe().expect("Scribe capability");
        let owner = scribe
            .try_acquire_memory(ScribeMemoryRequest {
                bytes: 1,
                category: crate::scribe::memory::MemoryCategory::Raw,
                shard: Some(0),
            })
            .expect("root admission");
        let observed = scribe.memory_epoch();
        let cancelled = {
            let scribe = scribe.clone();
            tokio::spawn(async move { scribe.wait_for_memory_change(observed).await })
        };
        cancelled.abort();
        assert!(
            cancelled.await.is_err(),
            "cancelled replay waiter owns no lease"
        );
        drop(owner);
        let advanced = scribe
            .wait_for_memory_change(observed)
            .await
            .expect("release advances the epoch");
        assert!(advanced > observed);
    }

    /// A release attribution mismatch poisons the sole root and retains bytes.
    #[test]
    fn scribe_release_mismatch_poisons_root_without_reusing_capacity() {
        let roles = BifrostRuntimeResources::composed_for_test(
            768 * MIB,
            512 * MIB as u64,
            [BifrostRole::Scribe, BifrostRole::Oracle],
        );
        let scribe = roles.scribe().expect("Scribe capability");
        let owner = scribe
            .try_acquire_memory(ScribeMemoryRequest {
                bytes: 8,
                category: crate::scribe::memory::MemoryCategory::Queued,
                shard: Some(1),
            })
            .expect("root admission");
        scribe
            .governor
            .inner
            .state
            .lock()
            .expect("test state lock")
            .scribe_category_bytes[crate::scribe::memory::MemoryCategory::Queued as usize] = 0;
        drop(owner);
        assert_eq!(
            roles.health().reason(),
            Some(BifrostResourcePoisonReason::Accounting)
        );
    }

    /// An admitted leader and follower govern no memory until their pools grow.
    ///
    /// Slot units are the whole admission cost on both paths: a class quantum is
    /// a ceiling and a partition-planning input, never resident memory. This
    /// pins both halves — zero governed bytes while idle, and exact shared-root
    /// reconciliation once each side actually reserves — because reintroducing a
    /// synthetic charge would make the pod refuse work it can still serve.
    ///
    /// # Panics
    ///
    /// Panics when admission, growth, or release does not reconcile exactly.
    #[test]
    fn oracle_leader_and_follower_govern_no_memory_until_their_pools_grow() {
        let roles = BifrostRuntimeResources::composed_for_test(
            768 * MIB,
            512 * MIB as u64,
            [BifrostRole::Oracle],
        );
        let oracle = roles.oracle().expect("Oracle capability");
        let baseline = oracle.snapshot().expect("idle baseline");
        let leader = oracle
            .try_acquire_query(analytical_query(0.0))
            .expect("analytical leader");
        let follower = oracle
            .try_acquire_worker(OracleWorkerClass::Analytical)
            .expect("analytical follower");
        let admitted = oracle.snapshot().expect("admitted snapshot");
        assert_eq!(
            admitted.oracle_query_memory_used_bytes, 0,
            "neither an admitted leader nor an admitted follower holds governed bytes"
        );
        assert_eq!(admitted.oracle_memory_used_bytes, 0);
        assert_eq!(
            admitted.oracle_query_slot_units,
            2 * ANALYTICAL_QUERY_SLOT_UNITS,
            "both sides charge the one aggregate slot ledger"
        );

        // The first actual consumer growth is the first memory charge, and it is
        // the only figure the shared root reports.
        let leader_pool = leader.memory_pool();
        let follower_pool = follower.memory_pool();
        let leader_bytes = MemoryConsumer::new("leader-growth").register(&leader_pool);
        let follower_bytes = MemoryConsumer::new("follower-growth").register(&follower_pool);
        leader_bytes
            .try_grow(8 * MIB)
            .expect("the leader view funds its own growth");
        follower_bytes
            .try_grow(4 * MIB)
            .expect("the follower view funds its own growth");
        let grown = oracle.snapshot().expect("grown snapshot");
        assert_eq!(grown.oracle_query_memory_used_bytes, 12 * MIB);
        assert_eq!(oracle.shared_memory_reserved(), 12 * MIB);

        drop((leader_bytes, follower_bytes));
        assert_eq!(oracle.shared_memory_reserved(), 0);
        drop((leader, follower));
        assert_eq!(
            oracle.snapshot().expect("released snapshot"),
            baseline,
            "release restores every counter exactly"
        );
        assert!(oracle.health().reason().is_none());
    }

    /// `memory_used` follows a running query's growth and shrink.
    ///
    /// Owner grant and release refresh every capacity series, but a query's
    /// memory moves between those two points. This holds one admitted query
    /// while its pool grows fallibly, shrinks, and then grows infallibly past
    /// its grant, reading `memory_used` at each step: a gauge that only moved at
    /// grant or release would show zero while bytes are held, and one that
    /// counted only governed bytes would stop at the grant.
    ///
    /// # Panics
    ///
    /// Panics when admission or growth fails or `memory_used` disagrees with
    /// the bytes the running query holds.
    #[test]
    fn oracle_memory_gauges_track_growth_while_a_query_is_held() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        metrics::with_local_recorder(&recorder, || {
            let roles = BifrostRuntimeResources::composed_for_test(
                768 * MIB,
                512 * MIB as u64,
                [BifrostRole::Oracle],
            );
            let oracle = roles.oracle().expect("Oracle capability");
            let query = oracle
                .try_acquire_query(interactive_query(0.0))
                .expect("query owner");
            let gauge = |kind: &str| {
                recorder
                    .snapshot()
                    .gauges
                    .get(&format!("bifrost_oracle_local_bytes{{kind=\"{kind}\"}}"))
                    .copied()
            };
            let pool = query.memory_pool();
            let bytes = MemoryConsumer::new("gauge-growth").register(&pool);

            bytes.try_grow(8 * MIB).expect("fallible growth is funded");
            assert_eq!(gauge("memory_used"), (8 * MIB).to_f64());
            bytes.shrink(8 * MIB);
            assert_eq!(
                gauge("memory_used"),
                Some(0.0),
                "shrink lowers the gauge while the query is still held"
            );

            bytes.grow(2048 * MIB);
            let grant = query
                .granted_memory_bytes
                .to_f64()
                .expect("grant is representable");
            assert!(
                gauge("memory_used").is_some_and(|value| value > grant),
                "infallible growth counts past the query's grant"
            );
            drop(bytes);
            drop(query);
            assert_eq!(gauge("memory_used"), Some(0.0));
        });
    }

    /// Query memory children nest under the admitted owner and charge the shared root.
    #[test]
    fn oracle_query_children_remain_nested_under_one_root_owner() {
        let roles = BifrostRuntimeResources::composed_for_test(
            768 * MIB,
            512 * MIB as u64,
            [BifrostRole::Scribe, BifrostRole::Oracle],
        );
        let oracle = roles.oracle().expect("Oracle capability");
        let query = oracle
            .try_acquire_query(interactive_query(0.0))
            .expect("query owner");
        let root_snapshot = oracle.snapshot().expect("root snapshot");
        let memory = query
            .try_split_memory("nested-query-test", 32 * MIB)
            .expect("nested memory child");
        assert_eq!(memory.bytes(), 32 * MIB);
        // Memory is not reserved at admission, so a memory child is a real
        // charge against the shared process root.
        let nested = oracle.snapshot().expect("nested snapshot");
        assert_eq!(
            nested.oracle_memory_used_bytes,
            root_snapshot.oracle_memory_used_bytes + 32 * MIB
        );
        drop(memory);
        drop(query);
        assert!(!oracle.snapshot().expect("released").oracle_query_active);
    }

    /// Exact interactive queries overlap until the slot ledger is full.
    ///
    /// Admission reserves neither memory nor disk, so slots are the only
    /// dimension that saturates and a refusal changes no counter.
    ///
    /// # Panics
    ///
    /// Panics when admission never saturates or a refusal or release mutates state.
    #[test]
    fn oracle_exact_queries_overlap_until_slot_exhaustion() {
        let roles = BifrostRuntimeResources::composed_for_test(
            1024 * MIB,
            512 * MIB as u64,
            [BifrostRole::Oracle],
        );
        let oracle = roles.oracle().expect("Oracle capability");
        let mut admitted = Vec::new();
        while let Ok(query) = oracle.try_acquire_query(interactive_query(0.0)) {
            admitted.push(query);
        }
        assert!(admitted.len() >= 2, "exact queries overlap");
        let occupied = oracle.snapshot().expect("aggregate snapshot");
        assert_eq!(
            occupied.oracle_query_memory_used_bytes, 0,
            "admission reserves no memory; the shared root is charged as consumers grow"
        );
        assert!(oracle.try_acquire_query(interactive_query(0.5)).is_err());
        assert_eq!(oracle.snapshot().expect("atomic refusal"), occupied);
        admitted.pop();
        admitted.push(
            oracle
                .try_acquire_query(interactive_query(0.5))
                .expect("release restores exact eligibility"),
        );
        drop(admitted);
        let released = oracle.snapshot().expect("released aggregate snapshot");
        assert_eq!(released.oracle_active_queries, 0);
        assert_eq!(released.oracle_memory_used_bytes, 0);
    }

    /// Query classes reject every noncanonical demand tuple without mutation.
    #[test]
    fn oracle_query_classes_require_their_exact_locked_quantum() {
        let roles = BifrostRuntimeResources::composed_for_test(
            1024 * MIB,
            1024 * MIB as u64,
            [BifrostRole::Oracle],
        );
        let oracle = roles.oracle().expect("Oracle capability");
        let baseline = oracle.snapshot().expect("empty governor snapshot");
        let malformed = [
            OracleResourceRequest {
                memory_bytes: 2 * ORACLE_PARTITION_MEMORY_BYTES,
                ..interactive_query(0.0)
            },
            OracleResourceRequest {
                slot_units: 2,
                ..interactive_query(0.0)
            },
            OracleResourceRequest {
                query_class: QueryClass::Analytical,
                memory_bytes: ORACLE_PARTITION_MEMORY_BYTES,
                spill_limit_bytes: (2 * ORACLE_PARTITION_MEMORY_BYTES) as u64,
                slot_units: 2,
                local_ratio: 0.0,
            },
            OracleResourceRequest {
                query_class: QueryClass::Analytical,
                memory_bytes: 2 * ORACLE_PARTITION_MEMORY_BYTES,
                spill_limit_bytes: ORACLE_PARTITION_MEMORY_BYTES as u64,
                slot_units: 2,
                local_ratio: 0.0,
            },
            OracleResourceRequest {
                query_class: QueryClass::Analytical,
                memory_bytes: 2 * ORACLE_PARTITION_MEMORY_BYTES,
                spill_limit_bytes: (2 * ORACLE_PARTITION_MEMORY_BYTES) as u64,
                slot_units: 1,
                local_ratio: 0.0,
            },
        ];

        for request in malformed {
            assert!(matches!(
                oracle.try_acquire_query(request),
                Err(BifrostResourceError::InvalidPlan { .. })
            ));
            assert_eq!(
                oracle.snapshot().expect("refusal snapshot"),
                baseline,
                "malformed {request:?} mutated root counters"
            );
        }
    }

    /// The dynamic grant divides a fixed budget by live slots and clamps both ends.
    ///
    /// # Panics
    ///
    /// Panics when the deterministic plan fixture cannot be composed.
    #[test]
    fn oracle_memory_grant_divides_a_fixed_budget_by_live_slots() {
        let mut policy = policy(&[BifrostRole::Oracle]);
        policy.oracle_query_slot_limit = Some(1024);
        let plan = BifrostRuntimeResources::from_snapshot(snapshot(1280 * MIB), policy)
            .expect("grant plan")
            .plan();
        let budget = plan.managed_memory_bytes;

        // Idle: the only live slots are the admitting query's own, so the whole
        // budget is available and the cap is what binds.
        assert_eq!(
            BifrostResourceGovernor::oracle_memory_grant(plan, 1, 1),
            ORACLE_PARTITION_MEMORY_BYTES,
            "an idle node grants the cap"
        );
        // A zero denominator must not divide by zero; it degrades to the cap.
        assert_eq!(
            BifrostResourceGovernor::oracle_memory_grant(plan, 1, 0),
            ORACLE_PARTITION_MEMORY_BYTES,
            "an empty slot ledger cannot divide by zero"
        );

        // Under load the grant is the plain quotient while it sits between the
        // clamps. Chosen so budget/slots lands strictly inside [floor, cap].
        let mid_slots = u32::try_from(budget / (64 * MIB)).expect("fixture slot count");
        let expected = budget / mid_slots as usize;
        assert!(
            (ORACLE_PARTITION_WORKING_MEMORY_BYTES..ORACLE_PARTITION_MEMORY_BYTES)
                .contains(&expected),
            "fixture must exercise the unclamped branch"
        );
        assert_eq!(
            BifrostResourceGovernor::oracle_memory_grant(plan, 1, mid_slots),
            expected
        );
        // An analytical query holding two slot units receives twice the share.
        assert_eq!(
            BifrostResourceGovernor::oracle_memory_grant(plan, 2, mid_slots),
            (2 * budget / mid_slots as usize).min(ORACLE_PARTITION_MEMORY_BYTES)
        );

        // Saturation: the floor holds even when the quotient falls below it, so
        // an admitted query always keeps enough memory for a partition to run.
        assert_eq!(
            BifrostResourceGovernor::oracle_memory_grant(plan, 1, u32::MAX),
            ORACLE_PARTITION_WORKING_MEMORY_BYTES,
            "the floor clamps a saturated node"
        );
    }

    /// Concurrent grants never oversubscribe the Oracle budget.
    ///
    /// This is the property that lets admission be decided by slot capacity
    /// alone, with no overcommit ratio and no kill-on-OOM backstop.
    ///
    /// # Panics
    ///
    /// Panics when the deterministic plan fixture cannot be composed.
    #[test]
    fn concurrent_oracle_grants_stay_inside_the_budget() {
        let mut policy = policy(&[BifrostRole::Oracle]);
        policy.oracle_query_slot_limit = Some(1024);
        let plan = BifrostRuntimeResources::from_snapshot(snapshot(1280 * MIB), policy)
            .expect("grant plan")
            .plan();
        let budget = plan.managed_memory_bytes;
        // Above the floor-clamp point the sum is bounded by the budget itself.
        // Below it the floor deliberately wins, and slot capacity — not the
        // grant — is what stops admission.
        let unclamped_limit = u32::try_from(budget / ORACLE_PARTITION_WORKING_MEMORY_BYTES)
            .expect("fixture slot count");
        for live in 1..=unclamped_limit {
            let total = BifrostResourceGovernor::oracle_memory_grant(plan, 1, live)
                .saturating_mul(live as usize);
            assert!(
                total <= budget,
                "{live} concurrent grants totalled {total} against a {budget} budget"
            );
        }
    }

    /// A smaller grant yields fewer partitions, smaller batches, and no hash join.
    #[test]
    fn oracle_session_shape_follows_the_grant() {
        let work_units = 64;
        let full = OracleSessionShape::for_grant(ORACLE_PARTITION_MEMORY_BYTES, 16, work_units);
        let floor =
            OracleSessionShape::for_grant(ORACLE_PARTITION_WORKING_MEMORY_BYTES, 2, work_units);

        assert!(
            floor.target_partitions < full.target_partitions,
            "a floor grant must not fan out as widely as a full grant"
        );
        // Batch size follows one partition's share, so the comparison is made
        // at a fixed partition count: a wider fan-out of the same grant buys
        // parallelism by giving each partition less memory, not more.
        let full_serial = OracleSessionShape::for_grant(ORACLE_PARTITION_MEMORY_BYTES, 1, 1);
        let floor_serial =
            OracleSessionShape::for_grant(ORACLE_PARTITION_WORKING_MEMORY_BYTES, 1, 1);
        assert!(
            floor_serial.batch_size < full_serial.batch_size,
            "a floor grant must hold less per batch than a full grant"
        );
        assert!(
            full.batch_size < full_serial.batch_size,
            "a wider fan-out of one grant must shrink each partition's batch"
        );
        assert!(full_serial.batch_size <= ORACLE_MAX_BATCH_SIZE);
        assert_eq!(floor.batch_size, ORACLE_MIN_BATCH_SIZE);

        assert!(
            full.prefer_hash_join,
            "a full grant has room for a hash join"
        );
        assert!(
            !floor.prefer_hash_join,
            "a floor grant must route away from the one operator that cannot spill"
        );

        // The batch size never leaves its bounds even for absurd grants.
        let tiny = OracleSessionShape::for_grant(1, 2, work_units);
        assert_eq!(tiny.batch_size, ORACLE_MIN_BATCH_SIZE);
        assert!(!tiny.prefer_hash_join);
        let huge = OracleSessionShape::for_grant(usize::MAX, 16, work_units);
        assert_eq!(huge.batch_size, ORACLE_MAX_BATCH_SIZE);
        assert!(huge.prefer_hash_join);
    }

    /// The built session configuration carries every knob the shape decided.
    #[test]
    fn oracle_session_config_carries_the_grant_derived_knobs() {
        let work_units = 64;
        let full = OracleSessionShape::for_grant(ORACLE_PARTITION_MEMORY_BYTES, 16, work_units);
        let floor =
            OracleSessionShape::for_grant(ORACLE_PARTITION_WORKING_MEMORY_BYTES, 2, work_units);

        let full_config = full.session_config();
        assert!(
            full_config.options().optimizer.prefer_hash_join,
            "a full-grant session leaves the hash join available"
        );
        assert_eq!(full_config.target_partitions(), full.target_partitions);
        assert_eq!(full_config.batch_size(), full.batch_size);
        // Held back per partition so a spilling sort can still merge its runs:
        // the merge reservation cannot spill, so a pool consumed entirely by
        // the sorting partitions fails the query instead of completing on disk.
        assert_eq!(
            full_config.options().execution.sort_spill_reservation_bytes,
            ORACLE_PARTITION_MEMORY_BYTES / full.target_partitions / 2
        );
        assert!(
            full_config
                .options()
                .execution
                .sort_spill_reservation_bytes
                .saturating_mul(full.target_partitions)
                < ORACLE_PARTITION_MEMORY_BYTES,
            "the merge reservations must not consume the whole grant"
        );

        let floor_config = floor.session_config();
        assert!(
            !floor_config.options().optimizer.prefer_hash_join,
            "a floor-grant session disables the one operator that cannot spill"
        );
        assert_eq!(
            floor_config
                .options()
                .execution
                .sort_spill_reservation_bytes,
            floor.sort_spill_reservation_bytes
        );
        assert_eq!(floor_config.target_partitions(), floor.target_partitions);
        assert_eq!(floor_config.batch_size(), floor.batch_size);
    }

    /// Analytical saturation preserves the derived Interactive slot floor.
    ///
    /// The floor is a slot rule, not a memory rule: nothing in the memory
    /// dimension reserves capacity for Interactive work, so this drives
    /// Analytical admission to its class maximum and proves an Interactive
    /// query still fits afterwards.
    #[test]
    fn analytical_capacity_preserves_one_interactive_quantum() {
        // Scratch and memory are deliberately generous so the slot ledger is the
        // dimension that binds; a memory refusal would hide the floor this covers.
        let mut policy = policy(&[BifrostRole::Oracle]);
        policy.oracle_query_slot_limit = Some(5);
        let mut probe = snapshot(1280 * MIB);
        probe.scratch_capacity_bytes = 64 * 1024 * MIB as u64;
        probe.scratch_available_bytes = 64 * 1024 * MIB as u64;
        let roles = BifrostRuntimeResources::from_snapshot(probe, policy)
            .expect("analytical reserve plan")
            .compose_roles()
            .expect("analytical reserve composition");
        let oracle = roles.oracle().expect("Oracle capability");
        let split = oracle.class_split();
        assert!(
            split.admits_analytical(),
            "the fixture must expose an analytical class to saturate"
        );
        let mut analytical = vec![
            oracle
                .try_acquire_query(analytical_query(0.0))
                .expect("analytical query below the class maximum"),
        ];
        while let Ok(query) = oracle.try_acquire_query(analytical_query(0.0)) {
            analytical.push(query);
        }
        let occupied = oracle.snapshot().expect("analytical owners at the maximum");
        assert!(
            oracle.live_slot_units() <= u64::from(split.analytical_max_units),
            "analytical saturation must never consume the interactive floor"
        );

        let refused = oracle.try_acquire_query(analytical_query(0.0));
        assert!(matches!(
            refused,
            Err(BifrostResourceError::Occupied { .. })
        ));
        assert_eq!(oracle.snapshot().expect("class refusal"), occupied);
        let interactive = oracle
            .try_acquire_query(interactive_query(0.0))
            .expect("protected interactive quantum remains available");
        let snapshot = oracle.snapshot().expect("mixed-class snapshot");
        assert_eq!(snapshot.oracle_interactive_queries, 1);
        assert_eq!(
            snapshot.oracle_analytical_queries as usize,
            analytical.len()
        );
        drop((analytical, interactive));
    }

    /// Query release is exact, idempotent, and fail-closed on surviving children.
    #[test]
    fn oracle_release_paths_are_exact_and_idempotent() {
        let roles = BifrostRuntimeResources::composed_for_test(
            1024 * MIB,
            512 * MIB as u64,
            [BifrostRole::Oracle],
        );
        let oracle = roles.oracle().expect("Oracle capability");
        let mut explicit = oracle
            .try_acquire_query(interactive_query(0.0))
            .expect("explicit owner");
        let memory = explicit
            .try_split_memory("release-child", 1)
            .expect("memory child");
        drop(memory);
        explicit.release().expect("explicit release");
        explicit.release().expect("idempotent release");
        drop(explicit);
        let dropped = oracle
            .try_acquire_query(interactive_query(0.0))
            .expect("drop owner");
        drop(dropped);
        assert_eq!(
            oracle
                .snapshot()
                .expect("ordinary release")
                .oracle_active_queries,
            0
        );

        let poisoned_roles = BifrostRuntimeResources::composed_for_test(
            1024 * MIB,
            512 * MIB as u64,
            [BifrostRole::Oracle],
        );
        let poisoned_oracle = poisoned_roles.oracle().expect("poison test capability");
        let owner = poisoned_oracle
            .try_acquire_query(interactive_query(0.0))
            .expect("poison owner");
        let child = owner
            .try_split_memory("surviving-child", 1)
            .expect("surviving child");
        drop(owner);
        assert_eq!(
            poisoned_roles.health().reason(),
            Some(BifrostResourcePoisonReason::Accounting)
        );
        drop(child);

        let underflow_roles = BifrostRuntimeResources::composed_for_test(
            1024 * MIB,
            512 * MIB as u64,
            [BifrostRole::Oracle],
        );
        let underflow_oracle = underflow_roles.oracle().expect("underflow test capability");
        let mut underflow_owner = underflow_oracle
            .try_acquire_query(interactive_query(0.0))
            .expect("underflow owner");
        underflow_oracle
            .governor
            .inner
            .state
            .lock()
            .expect("underflow state lock")
            .oracle_query_slot_units = 0;
        assert!(matches!(
            underflow_owner.release(),
            Err(BifrostResourceError::Poisoned { .. })
        ));
        assert_eq!(
            underflow_roles.health().reason(),
            Some(BifrostResourcePoisonReason::Accounting)
        );
        drop(underflow_owner);
    }

    /// A refused Oracle query mutates no counter and leaves the root usable.
    #[test]
    fn oracle_capability_refusal_is_atomic() {
        let roles = BifrostRuntimeResources::composed_for_test(
            768 * MIB,
            512 * MIB as u64,
            [BifrostRole::Scribe, BifrostRole::Oracle],
        );
        let oracle = roles.oracle().expect("Oracle capability");
        let held = oracle
            .try_acquire_query(interactive_query(0.0))
            .expect("first query owns one exact grant");
        let mut filled = Vec::new();
        while let Ok(query) = oracle.try_acquire_query(interactive_query(0.0)) {
            filled.push(query);
        }
        let occupied = oracle.snapshot().expect("occupied snapshot");
        assert!(oracle.try_acquire_query(interactive_query(0.0)).is_err());
        assert_eq!(
            oracle.snapshot().expect("post-refusal snapshot"),
            occupied,
            "a refusal must not mutate any counter"
        );
        drop(held);
        drop(filled);
        let released = oracle.snapshot().expect("released snapshot");
        assert_eq!(released.governed_memory_used_bytes, 0);
        assert!(!released.oracle_query_active);
        assert!(
            oracle.try_acquire_query(interactive_query(0.0)).is_ok(),
            "the root remains usable after a refusal"
        );
    }

    /// The Oracle runtime pool is the lease's own, and drop restores baselines.
    #[test]
    fn oracle_lease_pool_identity_and_drop_cleanup() {
        let roles = BifrostRuntimeResources::composed_for_test(
            1024 * MIB,
            512 * MIB as u64,
            [BifrostRole::Scribe, BifrostRole::Oracle],
        );
        let oracle = roles.oracle().expect("Oracle capability");
        let baseline = oracle.snapshot().expect("baseline");
        let query = oracle
            .try_acquire_query(interactive_query(0.0))
            .expect("query lease");
        let pool = query.memory_pool();
        assert!(
            Arc::ptr_eq(&pool, &query.memory_pool()),
            "the lease must issue one stable pool"
        );
        let reservation = MemoryConsumer::new("oracle-lease-identity").register(&pool);
        reservation
            .try_grow(query.granted_memory_bytes)
            .expect("the lease pool admits exactly its grant");
        assert!(reservation.try_grow(1).is_err());
        reservation.shrink(query.granted_memory_bytes);
        drop(query);
        assert_eq!(
            oracle.snapshot().expect("released"),
            baseline,
            "dropping the query lease must restore every baseline"
        );
    }
}
