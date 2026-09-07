//! Portable pod resource detection and cross-role Bifrost admission.
//!
//! [`BifrostResourceGovernor`] is the single process-local authority for the
//! memory and disposable scratch resources shared by Scribe, Oracle, and
//! Forge. Detection uses only process-visible operating-system interfaces;
//! deployment systems may reduce detected limits through absolute overrides
//! but are never part of the allocation policy.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
#[cfg(any(test, feature = "test-support"))]
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU8, Ordering as AtomicOrdering};
#[cfg(any(test, feature = "test-support"))]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use datafusion::error::DataFusionError;
#[cfg(any(test, feature = "test-support"))]
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
/// Protected process memory that Bifrost never allocates.
pub const MIN_UNMANAGED_RESERVE_BYTES: usize = 256 * MIB;
/// Protected memory floor for each enabled stateful serving role.
pub const ROLE_MEMORY_FLOOR_BYTES: usize = 256 * MIB;
/// Filesystem free space that disposable query spill never consumes.
pub const MIN_SCRATCH_FREE_BYTES: u64 = 256 * MIB as u64;
/// Largest `DataFusion` memory ceiling any single Oracle query may be granted.
///
/// This is a *cap on a derived grant*, not a reservation. Admission never debits
/// this amount from the shared elastic budget; see
/// [`ORACLE_PARTITION_WORKING_MEMORY_BYTES`] for what a query actually costs to
/// admit. Conflating the two pinned node concurrency at `budget / ceiling`,
/// which let a single analytical query reserve 1.25 GiB to scan a handful of
/// 5 KiB Parquet files and shed load at the lowest production rung.
pub const ORACLE_PARTITION_MEMORY_BYTES: usize = 256 * MIB;
/// Memory one Oracle slot unit charges against the shared elastic budget.
///
/// This serves two distinct jobs that happen to want the same number.
///
/// As an *admission charge* it is the working set one `DataFusion` execution
/// partition needs to make progress, so admitting a query reserves only what the
/// query genuinely needs to start rather than the largest envelope it might grow
/// into. As the *floor* of [`BifrostResourceGovernor::oracle_memory_grant`] it is
/// the smallest grant that still lets a partition run at all.
///
/// It is deliberately far smaller than [`ORACLE_PARTITION_MEMORY_BYTES`], which
/// caps a whole query's memory envelope. Dividing a query envelope by the
/// envelope quantum always yields one partition, which silently serializes every
/// Interactive query onto a single core. Partition parallelism is bounded by CPU
/// and by available work; memory only clamps it downward once a query envelope
/// can no longer give each partition room to run.
pub const ORACLE_PARTITION_WORKING_MEMORY_BYTES: usize = 32 * MIB;
/// Fewest Oracle slot units a node admits concurrently regardless of core count.
///
/// A slot unit is an admission unit, not a core: Vertica's `PLANNEDCONCURRENCY`
/// and Doris's query slots are both configured independently of CPU for the same
/// reason. The floor exists so a small-core node still absorbs the accepted
/// production workload — four Interactive units plus two Analytical queries at
/// two units each — on one node instead of refusing reads that callers then see
/// as failed queries rather than as backpressure.
pub const ORACLE_MIN_QUERY_SLOT_UNITS: usize = 8;
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
/// Concurrency is an explicit capacity decision. It is taken from
/// [`ResourcePlan::oracle_query_slot_limit`] when the deployment configured one,
/// and otherwise defaults to the Oracle budget divided by what a slot unit
/// charges, floored at [`ORACLE_MIN_QUERY_SLOT_UNITS`].
///
/// The divisor is [`ORACLE_PARTITION_WORKING_MEMORY_BYTES`] — what a slot unit
/// actually *charges* — not [`ORACLE_PARTITION_MEMORY_BYTES`], which is the
/// ceiling a query may grow into. That distinction is the whole point. Dividing
/// by the ceiling made concurrency a side effect of per-query generosity: raising
/// the ceiling so one query could use more memory silently reduced how many
/// queries the node would accept at all. Dividing by the charge asks the only
/// question admission cares about — how many queries fit — and leaves how much
/// memory each one may use to
/// [`BifrostResourceGovernor::oracle_memory_grant`], which shrinks the ceiling as
/// concurrency rises instead of refusing work.
///
/// There is deliberately no CPU term. An earlier draft clamped this to a
/// multiple of [`ResourcePlan::effective_cpu`], reasoning that a node cannot
/// schedule unbounded concurrent work. Measurement refuted it: on the
/// four-core R0 rung a `cpu * 4` clamp resolved to 16 units and still shed peer
/// work, while the unclamped divisor resolved to 78 and completed the same
/// workload with zero reservation refusals and zero read retries. A slot unit is
/// an admission unit, not a thread — intra-query parallelism is already bounded
/// by [`OracleSessionShape::target_partitions`], so bounding admission by cores
/// double-counts a limit the session config already applies. Nodes that need a
/// narrower ceiling configure one explicitly.
///
/// This value sizes both leader admission and the peer slot manager, which
/// matters because one node is usually both: it leads its own queries while
/// serving fragments for queries other nodes lead. Whether those two roles want
/// separate ceilings is unproven — R0's refusals are equally explained by a
/// single ceiling set too low, and the unclamped default clears them — so the
/// counter stays shared until a measurement distinguishes the two.
///
/// # Errors
///
/// Returns an invalid-plan error when checked Oracle capacity arithmetic cannot
/// produce a positive platform-sized slot count.
pub fn oracle_worker_slots(plan: ResourcePlan) -> Result<usize, BifrostResourceError> {
    if let Some(configured) = plan.oracle_query_slot_limit {
        return Ok(configured.max(1));
    }
    let budget = plan
        .oracle_floor_bytes
        .checked_add(plan.elastic_memory_bytes)
        .ok_or_else(accounting_overflow)?;
    Ok((budget / ORACLE_PARTITION_WORKING_MEMORY_BYTES).max(ORACLE_MIN_QUERY_SLOT_UNITS))
}

/// Bifrost roles that affect protected resource planning.
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
    /// Optional absolute memory cap; it may only reduce detection.
    pub memory_limit_bytes: Option<usize>,
    /// Optional absolute unmanaged reserve, never below the portable floor.
    pub unmanaged_reserve_bytes: Option<usize>,
    /// Optional absolute disposable scratch cap; it may only reduce detection.
    pub scratch_limit_bytes: Option<u64>,
    /// Optional effective CPU cap; it may only reduce detection.
    pub effective_cpu: Option<usize>,
    /// Optional absolute Forge compaction admission budget in bytes.
    ///
    /// `None` selects `floor(memory_limit_bytes * 4 / 5)`. Unlike the memory
    /// and CPU caps this is a deliberate capacity decision by the deployment
    /// rather than a detected process bound, so it may raise as well as reduce
    /// the derived value — but never past what the protected Scribe and Oracle
    /// floors leave free, which refuses boot instead of clamping.
    pub forge_compaction_memory_limit_bytes: Option<usize>,
    /// Optional explicit Oracle query slot-unit concurrency limit.
    ///
    /// `None` defaults to twice effective CPU, never below
    /// [`ORACLE_MIN_QUERY_SLOT_UNITS`]. Unlike the memory and CPU caps this one
    /// may raise as well as reduce the derived value: it is a deliberate
    /// capacity decision by the deployment, not a detected process bound.
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
    /// Oracle query scratch root.
    pub oracle_scratch: PathBuf,
}

/// Closed volume purpose used for bounded telemetry and diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BifrostVolumeClass {
    /// Durable write-ahead log occupancy.
    Wal,
    /// Durable Scribe staged-member occupancy.
    ScribeStage,
    /// Disposable Scribe persistence output.
    ScribeOutput,
    /// Disposable Oracle query spill.
    Oracle,
}

impl BifrostVolumeClass {
    /// Returns the closed telemetry label for this physical-volume purpose.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Wal => "wal",
            Self::ScribeStage => "scribe_stage",
            Self::ScribeOutput => "scribe_output",
            Self::Oracle => "oracle",
        }
    }

    /// Returns whether occupancy in this class survives a process restart.
    ///
    /// A durable class is reconciled from the filesystem at registration and
    /// released only by an explicit retirement; a disposable class is owned by
    /// live leases and returns its bytes when they drop.
    const fn is_durable(self) -> bool {
        matches!(self, Self::Wal | Self::ScribeStage)
    }

    /// Returns the closed role label responsible for this volume purpose.
    const fn role(self) -> &'static str {
        match self {
            Self::Wal | Self::ScribeStage | Self::ScribeOutput => "scribe",
            Self::Oracle => "oracle",
        }
    }
}

/// Publishes one closed physical-volume lifecycle transition.
fn record_volume_transition(class: BifrostVolumeClass, result: &'static str, current_bytes: u64) {
    metrics::counter!(
        "bifrost_resource_acquisitions_total",
        "role" => class.role(),
        "resource" => "volume",
        "result" => result,
        "volume_class" => class.as_str()
    )
    .increment(1);
    metrics::gauge!(
        "bifrost_resource_current_bytes",
        "role" => class.role(),
        "resource" => "volume",
        "volume_class" => class.as_str()
    )
    .set(current_bytes.to_f64().unwrap_or(f64::MAX));
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

/// One registered root resolved to its physical filesystem device.
#[derive(Debug, Clone)]
struct RegisteredVolumeRoot {
    /// Configured path used for live free-space probes.
    path: PathBuf,
    /// Stable device identity derived from filesystem metadata.
    device: u64,
    /// Closed ownership class.
    class: BifrostVolumeClass,
}

/// Exact per-device durable and provisional ownership.
#[derive(Debug, Default)]
struct VolumeDeviceState {
    /// Durable bytes reconciled or committed on this device, per class.
    ///
    /// Two classes are durable for different reasons: WAL bytes are the
    /// acknowledged rows themselves, and staged bytes are the local runs that
    /// let those WAL bytes retire. Both survive a restart, so both are
    /// reconciled from the filesystem at registration rather than assumed zero.
    durable_bytes: BTreeMap<BifrostVolumeClass, u64>,
    /// Durable growth admitted before its mutation completes, per class.
    provisional_bytes: BTreeMap<BifrostVolumeClass, u64>,
    /// Live disposable scratch leases.
    scratch_bytes: BTreeMap<BifrostVolumeClass, u64>,
    /// Whether accounting on this device remains trustworthy.
    poisoned: bool,
}

impl VolumeDeviceState {
    /// Returns committed durable bytes owned by one class.
    fn durable(&self, class: BifrostVolumeClass) -> u64 {
        self.durable_bytes.get(&class).copied().unwrap_or_default()
    }

    /// Returns admitted but uncommitted durable bytes owned by one class.
    fn provisional(&self, class: BifrostVolumeClass) -> u64 {
        self.provisional_bytes
            .get(&class)
            .copied()
            .unwrap_or_default()
    }

    /// Returns live disposable scratch bytes owned by one class.
    fn scratch(&self, class: BifrostVolumeClass) -> u64 {
        self.scratch_bytes.get(&class).copied().unwrap_or_default()
    }

    /// Returns the bytes one class currently owns under its own accounting.
    ///
    /// A durable class owns committed plus provisional growth; a disposable
    /// class owns its live leases. This is what telemetry reports and what a
    /// refusal diagnostic names.
    fn owned(&self, class: BifrostVolumeClass) -> u64 {
        if class.is_durable() {
            self.durable(class).saturating_add(self.provisional(class))
        } else {
            self.scratch(class)
        }
    }

    /// Sums every class's ownership on this device.
    ///
    /// # Errors
    ///
    /// Returns an accounting overflow when the device totals cannot be summed.
    fn total(&self) -> Result<u64, BifrostResourceError> {
        self.durable_bytes
            .values()
            .chain(self.provisional_bytes.values())
            .chain(self.scratch_bytes.values())
            .try_fold(0_u64, |total, bytes| total.checked_add(*bytes))
            .ok_or_else(accounting_overflow)
    }
}

/// Shared physical-volume authority grouped by filesystem device identity.
#[derive(Debug, Clone)]
pub struct BifrostVolumeGovernor {
    /// Registered role roots; aliases intentionally point at one device state.
    roots: Arc<BTreeMap<BifrostVolumeClass, RegisteredVolumeRoot>>,
    /// Exact per-device ownership serialized under one process lock.
    devices: Arc<Mutex<BTreeMap<u64, VolumeDeviceState>>>,
    /// Per-device configured disposable/durable occupancy ceiling.
    configured_limit_bytes: u64,
    /// Process lifecycle signal shared with memory accounting.
    health: BifrostResourceHealth,
}

impl BifrostVolumeGovernor {
    /// Registers roots, groups aliases by device, and reconciles retained WAL.
    ///
    /// # Errors
    ///
    /// Returns a typed unavailable or invalid-plan error when a root cannot be
    /// inspected, retained WAL cannot be measured, or no positive capacity
    /// remains after the 256 MiB physical free-space floor.
    pub fn register(
        roots: BifrostVolumeRoots,
        configured_limit_bytes: u64,
        health: BifrostResourceHealth,
    ) -> Result<Self, BifrostResourceError> {
        reconcile_scribe_scratch_namespace(&roots.scribe_output_scratch)?;
        let entries = [
            (BifrostVolumeClass::Wal, roots.wal),
            (BifrostVolumeClass::ScribeStage, roots.scribe_stage),
            (
                BifrostVolumeClass::ScribeOutput,
                roots.scribe_output_scratch,
            ),
            (BifrostVolumeClass::Oracle, roots.oracle_scratch),
        ];
        let mut registered = BTreeMap::new();
        let mut devices = BTreeMap::new();
        for (class, path) in entries {
            let metadata =
                fs::metadata(&path).map_err(|error| BifrostResourceError::Unavailable {
                    detail: format!("cannot inspect registered Bifrost volume root: {error}"),
                })?;
            #[cfg(unix)]
            let device = {
                use std::os::unix::fs::MetadataExt;
                metadata.dev()
            };
            #[cfg(not(unix))]
            let device = metadata.len();
            devices
                .entry(device)
                .or_insert_with(VolumeDeviceState::default);
            registered.insert(
                class,
                RegisteredVolumeRoot {
                    path,
                    device,
                    class,
                },
            );
        }
        let wal = registered.get(&BifrostVolumeClass::Wal).ok_or_else(|| {
            BifrostResourceError::InvalidPlan {
                detail: "WAL volume root was not registered".to_owned(),
            }
        })?;
        let retained = retained_wal_root_bytes(&wal.path)?;
        devices
            .get_mut(&wal.device)
            .ok_or_else(accounting_overflow)?
            .durable_bytes
            .insert(BifrostVolumeClass::Wal, retained);
        let stage_root = registered
            .get(&BifrostVolumeClass::ScribeStage)
            .ok_or_else(|| BifrostResourceError::InvalidPlan {
                detail: "Scribe stage volume root was not registered".to_owned(),
            })?;
        let staged = retained_stage_root_bytes(&stage_root.path)?;
        devices
            .get_mut(&stage_root.device)
            .ok_or_else(accounting_overflow)?
            .durable_bytes
            .insert(BifrostVolumeClass::ScribeStage, staged);
        let governor = Self {
            roots: Arc::new(registered),
            devices: Arc::new(Mutex::new(devices)),
            configured_limit_bytes,
            health,
        };
        for class in [
            BifrostVolumeClass::Wal,
            BifrostVolumeClass::ScribeStage,
            BifrostVolumeClass::ScribeOutput,
            BifrostVolumeClass::Oracle,
        ] {
            metrics::gauge!(
                "bifrost_resource_planned_bytes",
                "role" => class.role(),
                "resource" => "volume",
                "volume_class" => class.as_str()
            )
            .set(configured_limit_bytes.to_f64().unwrap_or(f64::MAX));
        }
        if governor.available_for(BifrostVolumeClass::Wal)? == 0 {
            return Err(BifrostResourceError::InvalidPlan {
                detail: "registered Bifrost volume has no usable capacity".to_owned(),
            });
        }
        Ok(governor)
    }

    /// Returns role-bound volume capabilities without exposing generic grants.
    #[must_use]
    pub fn capabilities(&self) -> BifrostVolumeCapabilities {
        BifrostVolumeCapabilities {
            wal: WalVolume {
                governor: self.clone(),
            },
            scribe_stage: StageVolume {
                governor: self.clone(),
            },
            scribe_output: ScratchVolume {
                governor: self.clone(),
                class: BifrostVolumeClass::ScribeOutput,
            },
            oracle: ScratchVolume {
                governor: self.clone(),
                class: BifrostVolumeClass::Oracle,
            },
        }
    }

    /// Returns exact ownership of one class for deterministic volume tests.
    ///
    /// The triple is the class's committed durable bytes, its provisional
    /// durable growth, and its disposable scratch bytes, so a test can prove a
    /// transition moved a charge between exactly those counters.
    ///
    /// # Errors
    ///
    /// Returns poison when the device lock or registered class cannot be read.
    #[cfg(test)]
    pub(crate) fn usage_for_test(
        &self,
        class: BifrostVolumeClass,
    ) -> Result<(u64, u64, u64), BifrostResourceError> {
        let root = self.roots.get(&class).ok_or_else(accounting_overflow)?;
        let devices = self
            .devices
            .lock()
            .map_err(|_| BifrostResourceError::Poisoned {
                detail: "volume state lock is poisoned".to_owned(),
            })?;
        let state = devices.get(&root.device).ok_or_else(accounting_overflow)?;
        Ok((
            state.durable(class),
            state.provisional(class),
            state.scratch(class),
        ))
    }

    /// Computes currently grantable bytes after configured and physical floors.
    fn available_for(&self, class: BifrostVolumeClass) -> Result<u64, BifrostResourceError> {
        let root = self
            .roots
            .get(&class)
            .ok_or_else(|| BifrostResourceError::InvalidPlan {
                detail: "volume class is not registered".to_owned(),
            })?;
        debug_assert_eq!(root.class, class);
        let available = filesystem_available_bytes(&root.path)?
            .checked_sub(MIN_SCRATCH_FREE_BYTES)
            .ok_or_else(|| BifrostResourceError::Occupied {
                detail: "physical volume cannot preserve the 256 MiB free-space floor".to_owned(),
            })?;
        Ok(self.configured_limit_bytes.min(available))
    }

    /// Atomically charges one device after a fresh physical free-space probe.
    ///
    /// `durable` selects which counter receives the charge: a durable class
    /// takes provisional growth that a later commit converts into retained
    /// occupancy, while a disposable class takes a live scratch lease that
    /// returns its bytes on drop.
    fn acquire(
        &self,
        class: BifrostVolumeClass,
        bytes: u64,
        durable: bool,
    ) -> Result<VolumeLease, BifrostResourceError> {
        if let Some(reason) = self.health.reason() {
            return Err(BifrostResourceError::Poisoned {
                detail: format!("resource health entered {reason:?}"),
            });
        }
        let available = self.available_for(class)?;
        let root = self
            .roots
            .get(&class)
            .ok_or_else(|| BifrostResourceError::InvalidPlan {
                detail: "volume class is not registered".to_owned(),
            })?;
        let mut devices = self
            .devices
            .lock()
            .map_err(|_| BifrostResourceError::Poisoned {
                detail: "volume state lock is poisoned".to_owned(),
            })?;
        let state = devices
            .get_mut(&root.device)
            .ok_or_else(accounting_overflow)?;
        if state.poisoned {
            return Err(BifrostResourceError::Poisoned {
                detail: "a prior physical-volume invariant failed".to_owned(),
            });
        }
        let used = state.total()?;
        let next = used.checked_add(bytes).ok_or_else(accounting_overflow)?;
        if next > self.configured_limit_bytes || next > available {
            record_volume_transition(class, "refused", state.owned(class));
            return Err(BifrostResourceError::Occupied {
                detail: "physical-volume request exceeds configured or live-free capacity"
                    .to_owned(),
            });
        }
        if durable {
            *state.provisional_bytes.entry(class).or_default() += bytes;
        } else {
            *state.scratch_bytes.entry(class).or_default() += bytes;
        }
        record_volume_transition(class, "acquired", state.owned(class));
        Ok(VolumeLease {
            governor: self.clone(),
            device: root.device,
            bytes,
            class,
            durable,
            retained: false,
        })
    }
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

/// Role-bound physical-volume capabilities issued by one registration.
#[derive(Debug)]
pub struct BifrostVolumeCapabilities {
    /// Durable WAL growth capability.
    pub wal: WalVolume,
    /// Durable Scribe staged-member capability.
    pub scribe_stage: StageVolume,
    /// Scribe output-scratch capability.
    pub scribe_output: ScratchVolume,
    /// Oracle scratch capability.
    pub oracle: ScratchVolume,
}

/// Converts one class's provisional growth into retained durable occupancy.
///
/// # Errors
///
/// Returns poison when the registered device or provisional counter no longer
/// covers this exact growth; the lease is retained rather than rolled back so
/// the divergence cannot be hidden by returning capacity twice.
fn commit_durable(mut lease: VolumeLease) -> Result<(), BifrostResourceError> {
    let class = lease.class;
    let mut devices =
        lease
            .governor
            .devices
            .lock()
            .map_err(|_| BifrostResourceError::Poisoned {
                detail: "volume state lock is poisoned".to_owned(),
            })?;
    let state = devices
        .get_mut(&lease.device)
        .ok_or_else(accounting_overflow)?;
    if state.provisional(class) < lease.bytes {
        state.poisoned = true;
        lease
            .governor
            .health
            .poison(BifrostResourcePoisonReason::Volume);
        return Err(BifrostResourceError::Poisoned {
            detail: "provisional durable growth diverged before commit".to_owned(),
        });
    }
    *state.provisional_bytes.entry(class).or_default() -= lease.bytes;
    let durable = state
        .durable(class)
        .checked_add(lease.bytes)
        .ok_or_else(accounting_overflow)?;
    state.durable_bytes.insert(class, durable);
    lease.retained = true;
    record_volume_transition(class, "committed", durable);
    Ok(())
}

/// Releases exact committed durable occupancy after its files are gone.
///
/// # Errors
///
/// Returns poison when reconciled durable ownership cannot cover the exact
/// retired length; no capacity is returned on mismatch.
fn retire_durable(
    governor: &BifrostVolumeGovernor,
    class: BifrostVolumeClass,
    bytes: u64,
) -> Result<(), BifrostResourceError> {
    let root = governor.roots.get(&class).ok_or_else(accounting_overflow)?;
    let mut devices = governor
        .devices
        .lock()
        .map_err(|_| BifrostResourceError::Poisoned {
            detail: "volume state lock is poisoned".to_owned(),
        })?;
    let state = devices
        .get_mut(&root.device)
        .ok_or_else(accounting_overflow)?;
    if state.durable(class) < bytes {
        state.poisoned = true;
        governor.health.poison(BifrostResourcePoisonReason::Volume);
        return Err(BifrostResourceError::Poisoned {
            detail: "durable volume retirement underflow".to_owned(),
        });
    }
    let remaining = state.durable(class) - bytes;
    state.durable_bytes.insert(class, remaining);
    record_volume_transition(class, "released", remaining);
    Ok(())
}

/// Non-generic durable Scribe staging capability.
///
/// Staged runs are the reason a WAL segment may retire, so their bytes are
/// accounted like WAL bytes rather than like scratch: they are reconciled from
/// the filesystem at registration, admitted before the runs are written,
/// committed when the member's record lands, and released only when the member
/// is retired after publication.
#[derive(Debug)]
pub struct StageVolume {
    /// Shared device-grouped authority.
    governor: BifrostVolumeGovernor,
}

impl StageVolume {
    /// Returns the registered durable staging root.
    ///
    /// # Errors
    ///
    /// Returns an invalid-plan error when the staging class is not registered,
    /// which registration prevents.
    pub fn root(&self) -> Result<&Path, BifrostResourceError> {
        self.governor
            .roots
            .get(&BifrostVolumeClass::ScribeStage)
            .map(|root| root.path.as_path())
            .ok_or_else(|| BifrostResourceError::InvalidPlan {
                detail: "Scribe stage volume class is not registered".to_owned(),
            })
    }

    /// Provisionally admits exact staged growth before the runs are written.
    ///
    /// # Errors
    ///
    /// Returns a typed refusal when the shared device cannot preserve both its
    /// configured ceiling and current physical free-space floor.
    pub fn try_reserve_growth(&self, bytes: u64) -> Result<StageGrowth, BifrostResourceError> {
        Ok(StageGrowth {
            lease: Some(
                self.governor
                    .acquire(BifrostVolumeClass::ScribeStage, bytes, true)?,
            ),
        })
    }

    /// Releases exact staged occupancy after a member's files are removed.
    ///
    /// # Errors
    ///
    /// Returns poison when reconciled staged ownership cannot cover the exact
    /// retired length.
    pub fn retire(&self, bytes: u64) -> Result<(), BifrostResourceError> {
        retire_durable(&self.governor, BifrostVolumeClass::ScribeStage, bytes)
    }
}

/// Provisional staged growth awaiting the record that makes it authoritative.
#[derive(Debug)]
pub struct StageGrowth {
    /// Shared provisional owner; `None` after durable commit.
    lease: Option<VolumeLease>,
}

impl StageGrowth {
    /// Reconciles the admitted estimate to measured bytes, then commits them.
    ///
    /// Staged bytes are admitted before encoding against the frozen bucket's
    /// retained Arrow size, which is an estimate rather than the encoded
    /// length. Committing therefore settles the difference first: a smaller
    /// encoding releases the unused provisional charge, and a larger one is
    /// re-admitted through the same ceiling and free-space floor as the
    /// original request, so a member never owns bytes the device never
    /// approved.
    ///
    /// # Errors
    ///
    /// Returns a typed refusal when measured bytes exceed the estimate and the
    /// device cannot admit the difference, and poison when the registered
    /// device or provisional counter no longer covers the admitted growth. A
    /// refusal leaves the estimate charged until this owner drops, so the
    /// caller must remove the measured files it could not commit.
    pub fn commit(mut self, measured_bytes: u64) -> Result<(), BifrostResourceError> {
        let mut lease = self.lease.take().ok_or_else(accounting_overflow)?;
        if measured_bytes > lease.bytes {
            let mut extra =
                lease
                    .governor
                    .acquire(lease.class, measured_bytes - lease.bytes, true)?;
            extra.retained = true;
            lease.bytes = measured_bytes;
        } else if measured_bytes < lease.bytes {
            release_provisional(&lease, lease.bytes - measured_bytes)?;
            lease.bytes = measured_bytes;
        }
        commit_durable(lease)
    }
}

/// Releases part of one lease's provisional durable charge before commit.
///
/// # Errors
///
/// Returns poison when the registered device or provisional counter no longer
/// covers the released difference; capacity is not returned on mismatch.
fn release_provisional(lease: &VolumeLease, bytes: u64) -> Result<(), BifrostResourceError> {
    let mut devices =
        lease
            .governor
            .devices
            .lock()
            .map_err(|_| BifrostResourceError::Poisoned {
                detail: "volume state lock is poisoned".to_owned(),
            })?;
    let state = devices
        .get_mut(&lease.device)
        .ok_or_else(accounting_overflow)?;
    if state.provisional(lease.class) < bytes {
        state.poisoned = true;
        lease
            .governor
            .health
            .poison(BifrostResourcePoisonReason::Volume);
        return Err(BifrostResourceError::Poisoned {
            detail: "provisional durable growth diverged before reconciliation".to_owned(),
        });
    }
    *state.provisional_bytes.entry(lease.class).or_default() -= bytes;
    record_volume_transition(lease.class, "released", state.owned(lease.class));
    Ok(())
}

/// Non-generic WAL volume capability.
#[derive(Debug)]
pub struct WalVolume {
    /// Shared device-grouped authority.
    governor: BifrostVolumeGovernor,
}

impl WalVolume {
    /// Provisionally admits exact WAL growth before write and fsync.
    ///
    /// # Errors
    ///
    /// Returns a typed refusal when the shared device cannot preserve both its
    /// configured ceiling and current physical free-space floor.
    pub fn try_reserve_growth(&self, bytes: u64) -> Result<WalVolumeGrowth, BifrostResourceError> {
        Ok(WalVolumeGrowth {
            lease: Some(
                self.governor
                    .acquire(BifrostVolumeClass::Wal, bytes, true)?,
            ),
        })
    }

    /// Releases exact durable occupancy after file removal and directory fsync.
    ///
    /// # Errors
    ///
    /// Returns poison when reconciled durable ownership cannot cover the exact
    /// retired file length; no capacity is returned on mismatch.
    pub fn retire(&self, bytes: u64) -> Result<(), BifrostResourceError> {
        retire_durable(&self.governor, BifrostVolumeClass::Wal, bytes)
    }

    /// Poisons shared health when failed WAL mutation cannot be reconciled.
    pub(crate) fn poison_divergence(&self) {
        self.governor
            .health
            .poison(BifrostResourcePoisonReason::Volume);
    }
}

/// Non-generic disposable scratch capability bound to one role root.
#[derive(Debug)]
pub struct ScratchVolume {
    /// Shared device-grouped authority.
    governor: BifrostVolumeGovernor,
    /// Closed role volume class.
    class: BifrostVolumeClass,
}

impl ScratchVolume {
    /// Acquires exact disposable occupancy until cleanup completes.
    ///
    /// # Errors
    ///
    /// Returns a typed refusal when the aliased physical device cannot cover
    /// the request while preserving configured and actual-free bounds.
    pub fn try_acquire(&self, bytes: u64) -> Result<ScratchLease, BifrostResourceError> {
        Ok(ScratchLease {
            lease: Some(self.governor.acquire(self.class, bytes, false)?),
        })
    }

    /// Creates the exact owned output directory one assembly claim merges into.
    ///
    /// A claim spans several shard generations, so its directory is named after
    /// the claim rather than after any one of them; restart reconciliation then
    /// attributes surviving residue to the publication that was interrupted.
    ///
    /// # Errors
    ///
    /// Returns a typed plan error for a non-Scribe capability or an unsafe
    /// stream component, a capacity refusal from the shared device governor, or
    /// an unavailable error when the exact owned directory cannot be created.
    pub fn create_scribe_claim(
        &self,
        stream: &str,
        claim: &str,
        bytes: u64,
    ) -> Result<ScribeClaimScratch, BifrostResourceError> {
        let component = safe_scratch_component(stream)?;
        let claim = safe_scratch_component(claim)?;
        self.create_owned_directory(&format!("{CLAIM_SCRATCH_PREFIX}{component}-{claim}"), bytes)
    }

    /// Creates one uniquely suffixed owned directory under the Scribe namespace.
    ///
    /// The returned owner removes only the directory it creates. Its name is
    /// rooted beneath the registered Scribe namespace and carries the identity
    /// its caller needs for restart reconciliation.
    ///
    /// # Errors
    ///
    /// Returns a typed plan error for a non-Scribe capability, a capacity
    /// refusal from the shared device governor, or an unavailable error when
    /// the exact owned directory cannot be created.
    fn create_owned_directory(
        &self,
        name: &str,
        bytes: u64,
    ) -> Result<ScribeClaimScratch, BifrostResourceError> {
        if self.class != BifrostVolumeClass::ScribeOutput {
            return Err(BifrostResourceError::InvalidPlan {
                detail: "Scribe claim scratch requires the Scribe output capability".to_owned(),
            });
        }
        let lease = self.try_acquire(bytes)?;
        let root = self
            .governor
            .roots
            .get(&self.class)
            .ok_or_else(accounting_overflow)?;
        let suffix = SCRATCH_NAMESPACE_SEQUENCE.fetch_add(1, AtomicOrdering::Relaxed);
        let path = root.path.join(format!("{name}-{suffix}"));
        fs::create_dir(&path).map_err(|error| BifrostResourceError::Unavailable {
            detail: format!("cannot create Scribe claim scratch: {error}"),
        })?;
        Ok(ScribeClaimScratch {
            path,
            namespace_root: root.path.clone(),
            lease: Some(lease),
            health: self.governor.health.clone(),
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

/// Claim-owned Scribe output directory and its exact physical charge.
#[derive(Debug)]
pub struct ScribeClaimScratch {
    /// Exact directory created for this claim.
    path: PathBuf,
    /// Registered parent used to prove cleanup containment and fsync completion.
    namespace_root: PathBuf,
    /// Charge released only after confirmed directory removal and parent fsync.
    lease: Option<ScratchLease>,
    /// Shared fail-stop signal poisoned after the final cleanup failure.
    health: BifrostResourceHealth,
}

impl ScribeClaimScratch {
    /// Returns the exact directory available to the claim writer.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Removes the exact claim directory and releases its physical charge.
    ///
    /// # Errors
    ///
    /// Returns a poisoned resource error after all bounded cleanup attempts
    /// fail. The exact charge remains retained and shared health is poisoned.
    pub fn cleanup(mut self) -> Result<(), BifrostResourceError> {
        self.cleanup_owned_prefix()
    }

    /// Runs the bounded exact-prefix cleanup protocol for explicit and drop paths.
    fn cleanup_owned_prefix(&mut self) -> Result<(), BifrostResourceError> {
        let namespace_root = self.namespace_root.clone();
        let path = self.path.clone();
        self.cleanup_owned_prefix_with(|| remove_exact_scratch_prefix(&namespace_root, &path))
    }

    /// Applies cleanup through an injectable exact-prefix operation for regression tests.
    fn cleanup_owned_prefix_with(
        &mut self,
        cleanup: impl FnMut() -> std::io::Result<()>,
    ) -> Result<(), BifrostResourceError> {
        if self.lease.is_none() {
            return Ok(());
        }
        let result = retry_scratch_cleanup(cleanup);
        if result.is_ok() {
            self.lease.take();
            return Ok(());
        }
        if let Some(lease) = self.lease.take() {
            lease.retain();
        }
        self.health.poison(BifrostResourcePoisonReason::Volume);
        Err(BifrostResourceError::Poisoned {
            detail: "Scribe claim scratch cleanup exhausted bounded retries".to_owned(),
        })
    }
}

impl Drop for ScribeClaimScratch {
    /// Retains ownership without filesystem work on cancellation and unwind.
    ///
    /// Explicit cleanup owns the blocking deletion and bounded retry boundary.
    /// A dropped owner may be running on a Tokio worker, so it cannot delete or
    /// sleep here. Retaining the exact charge and poisoning shared health keeps
    /// the residue visible to startup reconciliation and stops new admissions.
    fn drop(&mut self) {
        if let Some(lease) = self.lease.take() {
            lease.retain();
            self.health.poison(BifrostResourcePoisonReason::Volume);
            tracing::error!(
                operation = "scribe_claim_scratch_drop",
                outcome = "retained_poisoned",
                "Scribe claim scratch ownership dropped before explicit cleanup"
            );
        }
    }
}

/// Retains one failed scratch charge without running its ordinary release drop.
impl ScratchLease {
    fn retain(mut self) {
        if let Some(mut lease) = self.lease.take() {
            lease.retained = true;
        }
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

/// Provisional exact WAL growth committed only after durable mutation.
#[derive(Debug)]
pub struct WalVolumeGrowth {
    /// Shared provisional owner; `None` after durable commit.
    lease: Option<VolumeLease>,
}

impl WalVolumeGrowth {
    /// Converts provisional growth into retained durable WAL occupancy.
    ///
    /// # Errors
    ///
    /// Returns poison when the registered device or provisional counter no
    /// longer covers this exact growth.
    pub fn commit(mut self) -> Result<(), BifrostResourceError> {
        commit_durable(self.lease.take().ok_or_else(accounting_overflow)?)
    }

    /// Retains provisional ownership and poisons health after mutation divergence.
    pub(crate) fn retain_and_poison(mut self) {
        if let Some(mut lease) = self.lease.take() {
            lease.retained = true;
            lease
                .governor
                .health
                .poison(BifrostResourcePoisonReason::Volume);
        }
    }
}

/// Exact disposable scratch occupancy released after owned-prefix cleanup.
#[derive(Debug)]
pub struct ScratchLease {
    /// Shared lease retained until cleanup success or cancellation.
    lease: Option<VolumeLease>,
}

impl ScratchLease {
    /// Returns the exact charged disposable bytes.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.lease.as_ref().map_or(0, |lease| lease.bytes)
    }
}

/// Internal exact per-device ownership common to WAL and scratch shapes.
#[derive(Debug)]
struct VolumeLease {
    /// Device-grouped authority.
    governor: BifrostVolumeGovernor,
    /// Exact physical device charged by this owner.
    device: u64,
    /// Exact byte charge.
    bytes: u64,
    /// Closed capability class whose exact counter receives this ownership.
    class: BifrostVolumeClass,
    /// Whether this is provisional durable growth rather than disposable scratch.
    durable: bool,
    /// Committed durable or failed-cleanup ownership retained after this drops.
    retained: bool,
}

impl Drop for VolumeLease {
    /// Rolls back provisional durable growth or releases disposable scratch.
    fn drop(&mut self) {
        if self.retained {
            return;
        }
        let Ok(mut devices) = self.governor.devices.lock() else {
            self.governor
                .health
                .poison(BifrostResourcePoisonReason::Volume);
            return;
        };
        let Some(state) = devices.get_mut(&self.device) else {
            self.governor
                .health
                .poison(BifrostResourcePoisonReason::Volume);
            return;
        };
        let counter = if self.durable {
            state.provisional_bytes.entry(self.class).or_default()
        } else {
            state.scratch_bytes.entry(self.class).or_default()
        };
        if *counter < self.bytes {
            state.poisoned = true;
            self.governor
                .health
                .poison(BifrostResourcePoisonReason::Volume);
            return;
        }
        *counter -= self.bytes;
        let current = *counter;
        record_volume_transition(self.class, "released", current);
    }
}

/// Measures WAL segments and the flat, authorized Scribe staging namespace.
///
/// # Errors
///
/// Returns unavailable when a registered path cannot be read or measured.
fn retained_wal_root_bytes(root: &Path) -> Result<u64, BifrostResourceError> {
    retained_path_bytes(root, &root.join("staged"))
}

/// Measures retained Scribe staged bytes under the registered staging root.
///
/// The staging root is owned end to end by `ScribeHotStage`: every regular file
/// beneath it is either a staged run or a staged member record, and both are
/// durable until the member retires. Reconciliation therefore counts the whole
/// tree rather than a filename allowlist, so an interrupted stage cannot leave
/// bytes on the device that the governor does not own.
///
/// # Errors
///
/// Returns unavailable when the staging root cannot be read or measured.
fn retained_stage_root_bytes(root: &Path) -> Result<u64, BifrostResourceError> {
    let metadata =
        fs::symlink_metadata(root).map_err(|error| BifrostResourceError::Unavailable {
            detail: format!("cannot inspect retained stage path: {error}"),
        })?;
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    if !metadata.is_dir() {
        return Ok(0);
    }
    let mut total = 0_u64;
    for entry in fs::read_dir(root).map_err(|error| BifrostResourceError::Unavailable {
        detail: format!("cannot enumerate retained stage path: {error}"),
    })? {
        let entry = entry.map_err(|error| BifrostResourceError::Unavailable {
            detail: format!("cannot inspect retained stage entry: {error}"),
        })?;
        total = total
            .checked_add(retained_stage_root_bytes(&entry.path())?)
            .ok_or_else(accounting_overflow)?;
    }
    Ok(total)
}

/// Recursively measures retained WAL bytes without following symlinks.
///
/// Regular WAL files are counted outside the staged namespace. Inside the
/// staged namespace only Scribe's flat, closed filename set participates, so
/// an unrelated operator file cannot consume or release WAL capacity.
///
/// # Errors
///
/// Returns unavailable when a registered path cannot be read or measured.
fn retained_path_bytes(path: &Path, staged_root: &Path) -> Result<u64, BifrostResourceError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| BifrostResourceError::Unavailable {
            detail: format!("cannot inspect retained WAL path: {error}"),
        })?;
    if metadata.is_file() {
        let retained = if path.starts_with(staged_root) {
            path.parent() == Some(staged_root)
                && path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .is_some_and(is_authorized_staged_name)
        } else {
            path.extension().and_then(|value| value.to_str()) == Some("wal")
        };
        return Ok(if retained { metadata.len() } else { 0 });
    }
    if !metadata.is_dir() || (path.starts_with(staged_root) && path != staged_root) {
        return Ok(0);
    }
    let mut total = 0_u64;
    for entry in fs::read_dir(path).map_err(|error| BifrostResourceError::Unavailable {
        detail: format!("cannot enumerate retained WAL path: {error}"),
    })? {
        let entry = entry.map_err(|error| BifrostResourceError::Unavailable {
            detail: format!("cannot inspect retained WAL entry: {error}"),
        })?;
        total = total
            .checked_add(retained_path_bytes(&entry.path(), staged_root)?)
            .ok_or_else(accounting_overflow)?;
    }
    Ok(total)
}

/// Identifies one durable filename owned by the flat Scribe stage lifecycle.
fn is_authorized_staged_name(name: &str) -> bool {
    name.ends_with(".par.tmp")
        || name.ends_with(".parquet")
        || name.ends_with(".attempt.json")
        || name.ends_with(".winner")
        || name.ends_with(".publication.tmp")
        || name.ends_with(".publication.json")
}

/// Samples current filesystem-available bytes for one registered root.
///
/// The probed path is included in the failure detail. A registered volume root
/// that has been removed underneath a running pod reports the same errno as a
/// permission or mount fault, and without the path an operator cannot tell
/// which of the four volume classes failed or whether the directory simply no
/// longer exists.
///
/// # Errors
///
/// Returns unavailable when the live filesystem probe fails or overflows.
fn filesystem_available_bytes(path: &Path) -> Result<u64, BifrostResourceError> {
    let stats = statvfs(path).map_err(|error| BifrostResourceError::Unavailable {
        detail: format!(
            "cannot sample Bifrost volume free space at {}: {error}",
            path.display()
        ),
    })?;
    stats
        .f_bavail
        .checked_mul(stats.f_frsize)
        .ok_or_else(accounting_overflow)
}

/// Exact immutable capacity calculation shared by boot, telemetry, and tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourcePlan {
    /// Resolved process memory before the unmanaged reserve.
    pub memory_limit_bytes: usize,
    /// Effective CPU capacity used for query parallelism.
    pub effective_cpu: usize,
    /// Configured Oracle query slot-unit limit, or `None` to derive from CPU.
    ///
    /// Resolved through [`oracle_worker_slots`] rather than read directly, so
    /// the configured and derived paths cannot diverge.
    pub oracle_query_slot_limit: Option<usize>,
    /// Memory protected for the server and non-Bifrost work.
    pub unmanaged_reserve_bytes: usize,
    /// Total memory governed by Bifrost.
    pub managed_memory_bytes: usize,
    /// Protected Scribe floor, or zero when Scribe is inactive.
    pub scribe_floor_bytes: usize,
    /// Protected Oracle floor, or zero when Oracle is inactive.
    pub oracle_floor_bytes: usize,
    /// Immutable Forge compaction admission budget, zero when Forge is absent.
    ///
    /// Reserved once here rather than leased live: the worker-local queue
    /// charges its running plans against this one figure, so there is no second
    /// Forge accounting path in the shared governor to disagree with it.
    pub forge_compaction_memory_limit_bytes: usize,
    /// Memory available after every enabled role's protected floor and the
    /// reserved Forge compaction budget.
    pub elastic_memory_bytes: usize,
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
    /// Total live Scribe ownership, including its protected floor.
    pub scribe_memory_used_bytes: usize,
    /// Total live Oracle ownership, including its protected floor.
    pub oracle_memory_used_bytes: usize,
    /// Elastic memory held by Oracle.
    pub elastic_memory_used_bytes: usize,
    /// Disposable scratch held by Oracle.
    pub scratch_used_bytes: u64,
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
    /// Aggregate scratch retained specifically by Oracle query owners.
    pub oracle_query_scratch_used_bytes: u64,
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
    /// Exact positive root memory demand.
    pub memory_bytes: usize,
    /// Exact positive root scratch demand.
    pub scratch_bytes: u64,
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
    /// The memory term is the *admission charge*, not the ceiling the query may
    /// reach; that ceiling is derived per query at admission and is generally
    /// much larger. Scratch stays sized to the grant cap because it is genuinely
    /// consumed disk rather than a ceiling, so a query that spills must have
    /// reserved the space it spills into.
    #[must_use]
    pub fn for_class(query_class: QueryClass, local_ratio: f64) -> Self {
        let slot_units = match query_class {
            QueryClass::Interactive => 1,
            QueryClass::Analytical => 2,
        };
        Self {
            query_class,
            memory_bytes: ORACLE_PARTITION_WORKING_MEMORY_BYTES * slot_units as usize,
            scratch_bytes: ORACLE_PARTITION_MEMORY_BYTES as u64 * u64::from(slot_units),
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
    /// Returns the exact atomic root-memory charge for this worker class.
    ///
    /// This is the same per-slot-unit admission charge the leader envelope pays,
    /// deliberately so. The leader gate and the peer gate must agree on what one
    /// slot costs; when they disagreed, a query the leader had already committed
    /// to dispatching could still be refused by its own peers.
    fn memory_bytes(self) -> usize {
        ORACLE_PARTITION_WORKING_MEMORY_BYTES * self.slot_units() as usize
    }

    /// Returns the slot units one worker of this class occupies.
    fn slot_units(self) -> u32 {
        match self {
            Self::Interactive => 1,
            Self::Analytical => 2,
        }
    }
}

#[derive(Debug, Default)]
struct ResourceState {
    scribe_memory_used_bytes: usize,
    oracle_memory_used_bytes: usize,
    elastic_memory_used_bytes: usize,
    scratch_used_bytes: u64,
    oracle_active_queries: u32,
    oracle_interactive_queries: u32,
    oracle_analytical_queries: u32,
    oracle_query_slot_units: u32,
    oracle_query_memory_used_bytes: usize,
    oracle_query_scratch_used_bytes: u64,
    scribe_category_bytes: [usize; crate::scribe::memory::MEMORY_CATEGORY_COUNT],
    scribe_shard_bytes: BTreeMap<usize, usize>,
    memory_epoch: u64,
    poisoned: bool,
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
    /// Process-wide encoded transport-body admission inside unmanaged memory.
    transport: crate::gate::limits::BifrostTransportAdmission,
    /// Device-grouped physical-volume authority, present on live boot.
    volumes: Option<BifrostVolumeGovernor>,
}

impl BifrostRuntimeResources {
    /// Returns the sole process resource-health lifecycle signal.
    #[must_use]
    pub fn health(&self) -> BifrostResourceHealth {
        self.governor.inner.health.clone()
    }
    /// Detects process-visible resources and constructs the shared role graph.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError`] when detection or checked root-policy
    /// validation fails.
    pub fn detect(policy: BifrostResourcePolicy) -> Result<Self, BifrostResourceError> {
        Self::detect_with_transport_message_limit(
            policy,
            crate::gate::limits::BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES,
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
        let snapshot = detect_snapshot(&policy.scratch_root, policy.memory_limit_bytes)?;
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
            crate::gate::limits::BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES,
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
        let roots = policy.volume_roots.clone();
        let configured_limit = policy
            .scratch_limit_bytes
            .unwrap_or(snapshot.scratch_capacity_bytes);
        let root = BifrostResourceGovernor::from_snapshot(snapshot, policy)?;
        let volumes = roots
            .map(|roots| {
                BifrostVolumeGovernor::register(roots, configured_limit, root.inner.health.clone())
            })
            .transpose()?;
        let transport = crate::gate::limits::BifrostTransportAdmission::new(
            root.plan().unmanaged_reserve_bytes,
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
    /// # Panics
    ///
    /// Panics when the supplied observation cannot satisfy the policy, which in
    /// a unit test is an authoring error rather than a runtime condition.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn composed_for_test(
        memory_limit_bytes: usize,
        scratch_limit_bytes: u64,
        roles: impl IntoIterator<Item = BifrostRole>,
    ) -> BifrostRoleResources {
        let roles: BTreeSet<BifrostRole> = roles.into_iter().collect();
        let forge_compaction_memory_limit_bytes = forge_budget_for_test(&roles);
        let scratch_available_bytes = scratch_limit_bytes
            .checked_add(MIN_SCRATCH_FREE_BYTES)
            .expect("injected scratch observation must not overflow");
        Self::from_snapshot(
            SystemResourceSnapshot {
                memory_limit_bytes,
                effective_cpu: 4,
                scratch_capacity_bytes: scratch_available_bytes,
                scratch_available_bytes,
                memory_source: ResourceSource::Injected,
                cpu_source: ResourceSource::Injected,
            },
            BifrostResourcePolicy {
                roles,
                memory_limit_bytes: None,
                unmanaged_reserve_bytes: None,
                scratch_limit_bytes: Some(scratch_limit_bytes),
                effective_cpu: None,
                oracle_query_slot_limit: None,
                forge_compaction_memory_limit_bytes,
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
        let scribe = if plan.scribe_floor_bytes > 0 {
            Some(ScribeResources {
                governor: self.governor.clone(),
                volumes: self.volumes.clone(),
                follower_permits: Arc::new(Semaphore::new(plan.effective_cpu.max(1))),
            })
        } else {
            None
        };
        Ok(BifrostRoleResources {
            scribe,
            oracle: (plan.oracle_floor_bytes > 0).then(|| OracleResources {
                governor: self.governor.clone(),
                volumes: self.volumes.clone(),
            }),
            governor: self.governor.clone(),
            transport: self.transport.clone(),
            volumes: self.volumes.clone(),
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
    governor: BifrostResourceGovernor,
    /// Process-wide encoded body owner shared by HTTP and tonic surfaces.
    transport: crate::gate::limits::BifrostTransportAdmission,
    /// Device-grouped physical-volume authority registered during live boot.
    volumes: Option<BifrostVolumeGovernor>,
}

impl BifrostRoleResources {
    /// Returns fresh non-cloneable role-bound physical-volume capabilities.
    #[must_use]
    pub fn volume_capabilities(&self) -> Option<BifrostVolumeCapabilities> {
        self.volumes
            .as_ref()
            .map(BifrostVolumeGovernor::capabilities)
    }
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
    /// Physical WAL/output-scratch authority registered during live boot.
    volumes: Option<BifrostVolumeGovernor>,
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
    /// permit and a charge against the existing Scribe floor and shared elastic
    /// pool until the follower stream terminates. This adds no governor or root.
    ///
    /// # Errors
    ///
    /// Returns the existing root refusal when the requested positive quantum
    /// cannot be admitted without exceeding Scribe plus elastic capacity.
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
            bifrost_total_bytes: state
                .scribe_memory_used_bytes
                .saturating_add(state.oracle_memory_used_bytes),
            scribe_total_bytes: state.scribe_memory_used_bytes,
            scribe_limit_bytes: self.limit_bytes(),
            oracle_total_bytes: state.oracle_memory_used_bytes,
            oracle_limit_bytes: plan
                .oracle_floor_bytes
                .saturating_add(plan.elastic_memory_bytes),
            categories: state.scribe_category_bytes,
            cgroup_current_bytes: self.governor.cgroup_pressure().map(|(current, _)| current),
            cgroup_limit_bytes: self.governor.inner.cgroup_limit_bytes,
            ingress_occupancy_bytes: state.scribe_memory_used_bytes,
            ingress_limit_bytes: self.ingress_limit_bytes(),
            ingress_high_water_bytes: 0,
            ingress_low_water_bytes: 0,
        }
    }

    /// Returns fixed closed shard totals from root attribution.
    #[must_use]
    pub(crate) fn shard_snapshot(&self) -> [usize; 16] {
        let state = self
            .governor
            .inner
            .state
            .lock()
            .expect("Scribe root state lock");
        std::array::from_fn(|shard| {
            state
                .scribe_shard_bytes
                .get(&shard)
                .copied()
                .unwrap_or_default()
        })
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
    /// Returns a fresh handle on the durable Scribe staging volume.
    ///
    /// Kept separate from [`Self::volume_capabilities`] because staged runs are
    /// durable in a way output scratch is not: they authorize WAL retirement,
    /// so a caller that only needs encoding workspace must not be handed the
    /// capability that charges the staging floor.
    #[must_use]
    pub fn stage_volume(&self) -> Option<StageVolume> {
        self.volumes
            .as_ref()
            .map(|volumes| volumes.capabilities().scribe_stage)
    }

    /// Returns fresh non-cloneable WAL and output-scratch capabilities.
    #[must_use]
    pub fn volume_capabilities(&self) -> Option<(WalVolume, ScratchVolume)> {
        self.volumes.as_ref().map(|volumes| {
            let capabilities = volumes.capabilities();
            (capabilities.wal, capabilities.scribe_output)
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

    /// Returns the Scribe floor plus the root's shared elastic ceiling.
    #[must_use]
    pub(crate) fn limit_bytes(&self) -> usize {
        let plan = self.governor.plan();
        plan.scribe_floor_bytes
            .saturating_add(plan.elastic_memory_bytes)
    }

    /// Returns the full checked Scribe role ceiling for ingress ownership.
    ///
    /// This maximum envelope is distinct from per-bucket rotation targets and
    /// bounds a whole accepted request under the shared process-root governor.
    ///
    /// # Panics
    ///
    /// Panics if the validated root plan's Scribe floor and elastic allowance
    /// cannot be represented by `usize`.
    #[must_use]
    pub(crate) fn ingress_limit_bytes(&self) -> usize {
        let plan = self.governor.plan();
        plan.scribe_floor_bytes
            .checked_add(plan.elastic_memory_bytes)
            .expect("validated Scribe role ceiling must fit usize")
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
            bytes > self.ingress_limit_bytes()
                || snapshot
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
    /// Physical Oracle scratch authority registered during live boot.
    volumes: Option<BifrostVolumeGovernor>,
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

    /// Acquires one exact remote-worker quantum alongside other Oracle owners.
    ///
    /// The worker spends the shared Oracle floor first and borrows only the
    /// incremental elastic bytes required by its closed class quantum. Query,
    /// metadata, and sibling worker owners remain independently attributable in
    /// the same root ledger.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Occupied`] when the Oracle floor plus
    /// currently free elastic memory cannot cover the class quantum, or a
    /// poison/invalid-plan error when root accounting or role configuration is
    /// not trustworthy. Refusal changes no counters.
    pub fn try_acquire_worker(
        &self,
        class: OracleWorkerClass,
    ) -> Result<OracleWorkerResources, BifrostResourceError> {
        let lease = self
            .governor
            .try_acquire_oracle_memory(class.memory_bytes())?;
        let plan = self.governor.plan();
        let running_slot_units = self.governor.oracle_query_slot_units();
        let granted_memory_bytes = BifrostResourceGovernor::oracle_memory_grant(
            plan,
            class.slot_units(),
            running_slot_units.saturating_add(class.slot_units()),
        );
        let memory_pool = bounded_memory_pool(granted_memory_bytes);
        // Locality is zero here: a remote worker reads the files the leader
        // dispatched to it, so its partition ceiling comes from the grant it
        // was admitted with rather than from any caller-supplied hint.
        let admitted_target_partitions =
            oracle_target_partitions(plan.effective_cpu, 0.0, granted_memory_bytes)?;
        Ok(OracleWorkerResources {
            lease,
            memory_pool,
            granted_memory_bytes,
            admitted_target_partitions,
        })
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
    /// Memory is charged floor-first, scratch and slot units are additive, and
    /// analytical admission preserves one interactive quantum. The returned
    /// query owns a bounded `DataFusion` pool and nested scratch ceiling equal
    /// to this request rather than the role's remaining capacity.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Occupied`] when aggregate memory,
    /// scratch, slot units, physical scratch, or the protected interactive
    /// reserve cannot cover the exact request. Returns
    /// [`BifrostResourceError::InvalidPlan`] for zero, overflowing, inactive,
    /// or invalid partition demand, and a poison error for divergent root
    /// accounting. No root counter changes on root-admission refusal.
    pub fn try_acquire_query(
        &self,
        request: OracleResourceRequest,
    ) -> Result<OracleQueryResources, BifrostResourceError> {
        let mut resources = self.governor.try_acquire_oracle(request)?;
        if let Some(volumes) = &self.volumes {
            let capabilities = volumes.capabilities();
            resources.volume_scratch =
                Some(capabilities.oracle.try_acquire(resources.scratch_bytes)?);
        }
        Ok(resources)
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

#[derive(Debug)]
struct ResourceGovernorInner {
    plan: ResourcePlan,
    sources: ResolvedResourceSources,
    state: Mutex<ResourceState>,
    /// Lost-wakeup-safe notification paired with `ResourceState::memory_epoch`.
    memory_changed: Notify,
    /// Cgroup hard limit used by the live external-pressure tripwire.
    cgroup_limit_bytes: Option<usize>,
    /// Live cgroup usage cached for at most one second under the root owner.
    cgroup_current: Mutex<Option<(Instant, Option<usize>)>>,
    /// Lock-free first-poison signal observed by application supervision.
    health: BifrostResourceHealth,
}

/// Derives every enabled role floor and their checked aggregate.
///
/// # Errors
///
/// Returns [`BifrostResourceError::InvalidPlan`] if floor arithmetic exceeds
/// the platform's addressable memory range.
fn role_memory_floors(
    roles: &BTreeSet<BifrostRole>,
) -> Result<(usize, usize, usize), BifrostResourceError> {
    let scribe = usize::from(roles.contains(&BifrostRole::Scribe)) * ROLE_MEMORY_FLOOR_BYTES;
    let oracle = usize::from(roles.contains(&BifrostRole::Oracle)) * ROLE_MEMORY_FLOOR_BYTES;
    let protected = scribe.checked_add(oracle).ok_or_else(accounting_overflow)?;
    Ok((scribe, oracle, protected))
}

/// One executable rewrite working set, the budget test observations name.
///
/// The production default is four fifths of the memory limit and is
/// deliberately unclamped, so a node that also protects the Scribe and Oracle
/// floors cannot afford it and refuses to plan — exactly as a real co-located
/// deployment does until it configures one. A test observation therefore names
/// this figure instead, which is the smallest budget on which one rewrite can
/// execute and is what every constitutional topology floor asserted below is
/// the sum of.
#[cfg(test)]
pub(crate) const FORGE_TEST_BUDGET_BYTES: usize = 64 * MIB;

/// The Forge budget a test observation must name to plan at all.
///
/// `None` when Forge is not enabled, where the production path fixes the budget
/// at zero. The budget arithmetic itself is pinned by
/// `forge_compaction_budget_preserves_role_floors` against explicit policies,
/// never through this helper.
#[cfg(test)]
fn forge_budget_for_test(roles: &BTreeSet<BifrostRole>) -> Option<usize> {
    roles
        .contains(&BifrostRole::Forge)
        .then_some(FORGE_TEST_BUDGET_BYTES)
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

/// Resolves the immutable Forge compaction budget and the elastic remainder.
///
/// The default is four fifths of the resolved process memory, which is the
redacted
/// worker. `safe_bytes` is what remains after the protected Scribe and Oracle
/// floors, so a dedicated Forge pod and a co-located `All` pod use one formula
/// while `All` still cannot spend either protected floor.
///
/// There is deliberately no clamp. A budget that does not fit is a deployment
/// that asked for something the pod cannot honour, and silently shrinking it
/// would let an operator believe Forge was admitted memory it never had.
///
/// # Errors
///
/// Returns [`BifrostResourceError::InvalidPlan`] when the default computation
/// overflows, or when Forge is enabled and the selected budget is zero or
/// exceeds `safe_bytes`.
fn forge_compaction_budget(
    forge_enabled: bool,
    memory_limit_bytes: usize,
    safe_bytes: usize,
    override_bytes: Option<usize>,
) -> Result<(usize, usize), BifrostResourceError> {
    if !forge_enabled {
        return Ok((0, safe_bytes));
    }
    let default_bytes = memory_limit_bytes
        .checked_mul(4)
        .ok_or_else(accounting_overflow)?
        / 5;
    let selected = override_bytes.unwrap_or(default_bytes);
    if selected == 0 || selected > safe_bytes {
        return Err(BifrostResourceError::InvalidPlan {
            detail: format!(
                "Forge compaction memory budget {selected} must be positive and at most \
                 the {safe_bytes} bytes left by the protected Scribe and Oracle floors"
            ),
        });
    }
    let elastic = safe_bytes
        .checked_sub(selected)
        .ok_or_else(accounting_overflow)?;
    Ok((selected, elastic))
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
        let memory_limit_bytes = cap_positive(
            snapshot.memory_limit_bytes,
            policy.memory_limit_bytes,
            "memory",
        )?;
        let effective_cpu = cap_positive(snapshot.effective_cpu, policy.effective_cpu, "CPU")?;
        let oracle_query_slot_limit = policy.oracle_query_slot_limit;
        let unmanaged_reserve_bytes = policy
            .unmanaged_reserve_bytes
            .unwrap_or(MIN_UNMANAGED_RESERVE_BYTES);
        if unmanaged_reserve_bytes < MIN_UNMANAGED_RESERVE_BYTES {
            return Err(BifrostResourceError::InvalidPlan {
                detail: format!(
                    "unmanaged reserve {unmanaged_reserve_bytes} is below {MIN_UNMANAGED_RESERVE_BYTES}"
                ),
            });
        }
        let managed_memory_bytes = memory_limit_bytes
            .checked_sub(unmanaged_reserve_bytes)
            .ok_or_else(|| BifrostResourceError::InvalidPlan {
                detail: format!(
                    "memory {memory_limit_bytes} cannot cover reserve {unmanaged_reserve_bytes}"
                ),
            })?;
        let (scribe_floor_bytes, oracle_floor_bytes, protected) =
            role_memory_floors(&policy.roles)?;
        let safe_forge_budget_bytes =
            managed_memory_bytes.checked_sub(protected).ok_or_else(|| {
                BifrostResourceError::InvalidPlan {
                    detail: format!("managed memory {managed_memory_bytes} cannot cover enabled role floors {protected}"),
                }
            })?;
        let (forge_compaction_memory_limit_bytes, elastic_memory_bytes) = forge_compaction_budget(
            policy.roles.contains(&BifrostRole::Forge),
            memory_limit_bytes,
            safe_forge_budget_bytes,
            policy.forge_compaction_memory_limit_bytes,
        )?;
        let (scratch_limit_bytes, available_after_floor) = scratch_budget(&snapshot, &policy)?;
        drop(policy.scratch_root);
        let root = Self {
            inner: Arc::new(ResourceGovernorInner {
                plan: ResourcePlan {
                    memory_limit_bytes,
                    effective_cpu,
                    oracle_query_slot_limit,
                    unmanaged_reserve_bytes,
                    managed_memory_bytes,
                    scribe_floor_bytes,
                    oracle_floor_bytes,
                    forge_compaction_memory_limit_bytes,
                    elastic_memory_bytes,
                    scratch_limit_bytes,
                },
                sources: ResolvedResourceSources {
                    memory: if policy
                        .memory_limit_bytes
                        .is_some_and(|limit| limit <= snapshot.memory_limit_bytes)
                    {
                        ResourceSource::Override
                    } else {
                        snapshot.memory_source
                    },
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
                state: Mutex::new(ResourceState::default()),
                memory_changed: Notify::new(),
                cgroup_limit_bytes: crate::scribe::memory::read_cgroup_limit(),
                cgroup_current: Mutex::new(None),
                health: BifrostResourceHealth::default(),
            }),
        };
        root.record_plan_metrics();
        Ok(root)
    }

    /// Publishes immutable closed-role memory and scratch plan gauges.
    fn record_plan_metrics(&self) {
        let plan = self.plan();
        let planned = [
            ("unmanaged", "memory", plan.unmanaged_reserve_bytes),
            ("scribe", "memory", plan.scribe_floor_bytes),
            ("oracle", "memory", plan.oracle_floor_bytes),
            (
                "forge_compaction",
                "memory",
                plan.forge_compaction_memory_limit_bytes,
            ),
            ("elastic", "memory", plan.elastic_memory_bytes),
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
        metrics::gauge!(
            "bifrost_resource_planned_bytes",
            "role" => "transport",
            "resource" => "memory"
        )
        .set(plan.unmanaged_reserve_bytes.to_f64().unwrap_or(f64::MAX));
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

    /// Captures exact live elastic and scratch ownership.
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
            scribe_memory_used_bytes: state.scribe_memory_used_bytes,
            oracle_memory_used_bytes: state.oracle_memory_used_bytes,
            elastic_memory_used_bytes: state.elastic_memory_used_bytes,
            scratch_used_bytes: state.scratch_used_bytes,
            oracle_active_queries: state.oracle_active_queries,
            oracle_interactive_queries: state.oracle_interactive_queries,
            oracle_analytical_queries: state.oracle_analytical_queries,
            oracle_query_slot_units: state.oracle_query_slot_units,
            oracle_query_memory_used_bytes: state.oracle_query_memory_used_bytes,
            oracle_query_scratch_used_bytes: state.oracle_query_scratch_used_bytes,
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
        let role_total = state
            .scribe_memory_used_bytes
            .checked_add(state.oracle_memory_used_bytes)
            .ok_or_else(accounting_overflow)?;
        let plan = self.plan();
        let expected_elastic = state
            .scribe_memory_used_bytes
            .saturating_sub(plan.scribe_floor_bytes)
            .checked_add(
                state
                    .oracle_memory_used_bytes
                    .saturating_sub(plan.oracle_floor_bytes),
            )
            .ok_or_else(accounting_overflow)?;
        if category_total != state.scribe_memory_used_bytes
            || shard_total > state.scribe_memory_used_bytes
            || role_total > plan.managed_memory_bytes
            || expected_elastic != state.elastic_memory_used_bytes
        {
            // Emit the operands: which of the four identities broke is the
            // whole diagnosis, and it is unrecoverable from the error string.
            tracing::error!(
                category_total,
                shard_total,
                role_total,
                expected_elastic,
                scribe_memory_used_bytes = state.scribe_memory_used_bytes,
                elastic_memory_used_bytes = state.elastic_memory_used_bytes,
                managed_memory_bytes = plan.managed_memory_bytes,
                "Scribe root attribution does not reconcile to live ownership"
            );
            self.inner
                .health
                .poison(BifrostResourcePoisonReason::Accounting);
            self.inner.memory_changed.notify_waiters();
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
        metrics::gauge!("bifrost_resource_memory_bytes", "kind" => "elastic_total")
            .set(as_f64(snapshot.plan.elastic_memory_bytes));
        metrics::gauge!("bifrost_resource_memory_bytes", "kind" => "elastic_used")
            .set(as_f64(snapshot.elastic_memory_used_bytes));
        metrics::gauge!("bifrost_resource_memory_bytes", "kind" => "scribe_floor")
            .set(as_f64(snapshot.plan.scribe_floor_bytes));
        metrics::gauge!("bifrost_resource_memory_bytes", "kind" => "scribe_used")
            .set(as_f64(snapshot.scribe_memory_used_bytes));
        metrics::gauge!("bifrost_resource_memory_bytes", "kind" => "oracle_floor")
            .set(as_f64(snapshot.plan.oracle_floor_bytes));
        metrics::gauge!("bifrost_resource_memory_bytes", "kind" => "oracle_used")
            .set(as_f64(snapshot.oracle_memory_used_bytes));
        metrics::gauge!("bifrost_resource_memory_bytes", "kind" => "forge_compaction_budget")
            .set(as_f64(snapshot.plan.forge_compaction_memory_limit_bytes));
        metrics::gauge!("bifrost_resource_scratch_bytes", "kind" => "total").set(
            snapshot
                .plan
                .scratch_limit_bytes
                .to_f64()
                .unwrap_or(f64::MAX),
        );
        metrics::gauge!("bifrost_resource_scratch_bytes", "kind" => "used")
            .set(snapshot.scratch_used_bytes.to_f64().unwrap_or(f64::MAX));
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
        if plan.scribe_floor_bytes == 0 {
            return Err(BifrostResourceError::InvalidPlan {
                detail: "Scribe resources requested while the role is inactive".to_owned(),
            });
        }
        let next = state
            .scribe_memory_used_bytes
            .checked_add(request.bytes)
            .ok_or_else(accounting_overflow)?;
        let prior_borrow = state
            .scribe_memory_used_bytes
            .saturating_sub(plan.scribe_floor_bytes);
        let next_borrow = next.saturating_sub(plan.scribe_floor_bytes);
        let added_elastic = next_borrow - prior_borrow;
        let next_elastic = state
            .elastic_memory_used_bytes
            .checked_add(added_elastic)
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
            record_memory_transition("scribe", "refused", state.scribe_memory_used_bytes);
            return Err(BifrostResourceError::Occupied {
                detail: "Scribe request exceeds the ingress sublimit".to_owned(),
            });
        }
        if next_elastic > plan.elastic_memory_bytes {
            record_memory_transition("scribe", "refused", state.scribe_memory_used_bytes);
            return Err(BifrostResourceError::Occupied {
                detail: "Scribe request exceeds protected floor plus free elastic memory"
                    .to_owned(),
            });
        }
        state.scribe_memory_used_bytes = next;
        state.elastic_memory_used_bytes = next_elastic;
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
        if plan.oracle_floor_bytes == 0 {
            return Err(BifrostResourceError::InvalidPlan {
                detail: "Oracle resources requested while the role is inactive".to_owned(),
            });
        }
        if request.memory_bytes == 0 || request.scratch_bytes == 0 || request.slot_units == 0 {
            return Err(BifrostResourceError::InvalidPlan {
                detail: "Oracle query demands must be positive".to_owned(),
            });
        }
        let exact = OracleResourceRequest::for_class(request.query_class, request.local_ratio);
        if (
            request.memory_bytes,
            request.scratch_bytes,
            request.slot_units,
        ) != (exact.memory_bytes, exact.scratch_bytes, exact.slot_units)
        {
            return Err(BifrostResourceError::InvalidPlan {
                detail: "Oracle query demand must match its class quantum".to_owned(),
            });
        }
        Ok(())
    }

    /// Returns Oracle slot units currently held by live query owners.
    ///
    /// Returns zero when the shared ledger is poisoned. A grant sized from a
    /// zero denominator is the cap, which is the correct degradation: a poisoned
    /// ledger already fails admission, and a ceiling that is too generous cannot
    /// admit work that slot capacity has not already allowed.
    fn oracle_query_slot_units(&self) -> u32 {
        self.lock_state()
            .map_or(0, |state| state.oracle_query_slot_units)
    }

    /// Derives the memory ceiling one admitted query may grow into.
    ///
    /// The grant is `oracle_budget * query_slots / sum(running_slots)`, clamped
    /// to `[ORACLE_PARTITION_WORKING_MEMORY_BYTES, ORACLE_PARTITION_MEMORY_BYTES]`.
    /// `running_slots` must already include the admitting query's own units, so
    /// an otherwise idle node grants the cap rather than dividing by zero.
    ///
    /// Two properties matter and neither is incidental.
    ///
    /// The numerator is the *configured* Oracle budget, never currently free
    /// memory. Dividing free memory would make two identical queries receive
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
        let budget = plan
            .oracle_floor_bytes
            .saturating_add(plan.elastic_memory_bytes);
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
    ) -> Result<OracleQueryResources, BifrostResourceError> {
        let mut state = self.lock_state()?;
        let plan = self.plan();
        Self::validate_oracle_request(plan, request)?;
        let next_memory = state
            .oracle_memory_used_bytes
            .checked_add(request.memory_bytes)
            .ok_or_else(accounting_overflow)?;
        let prior_borrow = state
            .oracle_memory_used_bytes
            .saturating_sub(plan.oracle_floor_bytes);
        let next_borrow = next_memory.saturating_sub(plan.oracle_floor_bytes);
        let added_elastic = next_borrow
            .checked_sub(prior_borrow)
            .ok_or_else(accounting_overflow)?;
        let next_elastic = state
            .elastic_memory_used_bytes
            .checked_add(added_elastic)
            .ok_or_else(accounting_overflow)?;
        let next_scratch = state
            .scratch_used_bytes
            .checked_add(request.scratch_bytes)
            .ok_or_else(accounting_overflow)?;
        let next_slots = state
            .oracle_query_slot_units
            .checked_add(request.slot_units)
            .ok_or_else(accounting_overflow)?;
        let slot_limit =
            u32::try_from(oracle_worker_slots(plan)?).map_err(|_| accounting_overflow())?;
        let role_ceiling = plan
            .oracle_floor_bytes
            .checked_add(
                plan.elastic_memory_bytes
                    .checked_sub(state.elastic_memory_used_bytes)
                    .ok_or_else(|| self.poison_locked(&mut state, "elastic memory underflow"))?,
            )
            .and_then(|free| free.checked_add(prior_borrow))
            .ok_or_else(accounting_overflow)?;
        // The reserve protects one *admission* of an interactive query, not one
        // grant cap. Holding back a whole cap would reserve eight slot-unit
        // quanta to protect a query that charges one, which is the same
        // reservation-versus-ceiling conflation this admission path removed.
        let analytical_ceiling = role_ceiling.saturating_sub(ORACLE_PARTITION_WORKING_MEMORY_BYTES);
        if next_elastic > plan.elastic_memory_bytes
            || next_scratch > plan.scratch_limit_bytes
            || next_slots > slot_limit
            || (request.query_class == QueryClass::Analytical && next_memory > analytical_ceiling)
        {
            record_memory_transition("oracle", "refused", state.oracle_memory_used_bytes);
            return Err(BifrostResourceError::Occupied {
                detail: "Oracle query exceeds aggregate memory, scratch, slot, or protected interactive capacity".to_owned(),
            });
        }
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
        let next_query_memory = state
            .oracle_query_memory_used_bytes
            .checked_add(request.memory_bytes)
            .ok_or_else(accounting_overflow)?;
        let next_query_scratch = state
            .oracle_query_scratch_used_bytes
            .checked_add(request.scratch_bytes)
            .ok_or_else(accounting_overflow)?;
        state.elastic_memory_used_bytes = next_elastic;
        state.oracle_memory_used_bytes = next_memory;
        state.scratch_used_bytes = next_scratch;
        state.oracle_query_slot_units = next_slots;
        state.oracle_active_queries = next_active;
        state.oracle_interactive_queries = next_interactive;
        state.oracle_analytical_queries = next_analytical;
        state.oracle_query_memory_used_bytes = next_query_memory;
        state.oracle_query_scratch_used_bytes = next_query_scratch;
        record_memory_transition("oracle", "acquired", next_memory);
        let memory_pool = bounded_memory_pool(granted_memory_bytes);
        Ok(OracleQueryResources {
            query_class: request.query_class,
            memory_bytes: request.memory_bytes,
            granted_memory_bytes,
            scratch_bytes: request.scratch_bytes,
            slot_units: request.slot_units,
            target_partitions,
            memory_pool,
            nested_scratch_used_bytes: Arc::new(Mutex::new(0)),
            governor: self.clone(),
            released: false,
            volume_scratch: None,
        })
    }

    /// Acquires an exact Oracle-role memory owner, spending its floor first.
    fn try_acquire_oracle_memory(
        &self,
        bytes: usize,
    ) -> Result<OracleMemoryLease, BifrostResourceError> {
        let mut state = self.lock_state()?;
        let plan = self.plan();
        if plan.oracle_floor_bytes == 0 {
            return Err(BifrostResourceError::InvalidPlan {
                detail: "Oracle resources requested while the role is inactive".to_owned(),
            });
        }
        let next = state
            .oracle_memory_used_bytes
            .checked_add(bytes)
            .ok_or_else(accounting_overflow)?;
        let prior_borrow = state
            .oracle_memory_used_bytes
            .saturating_sub(plan.oracle_floor_bytes);
        let next_borrow = next.saturating_sub(plan.oracle_floor_bytes);
        let added_elastic = next_borrow - prior_borrow;
        let next_elastic = state
            .elastic_memory_used_bytes
            .checked_add(added_elastic)
            .ok_or_else(accounting_overflow)?;
        if next_elastic > plan.elastic_memory_bytes {
            record_memory_transition("oracle", "refused", state.oracle_memory_used_bytes);
            return Err(BifrostResourceError::Occupied {
                detail: "Oracle request exceeds protected floor plus free elastic memory"
                    .to_owned(),
            });
        }
        state.oracle_memory_used_bytes = next;
        state.elastic_memory_used_bytes = next_elastic;
        record_memory_transition("oracle", "acquired", next);
        Ok(OracleMemoryLease {
            bytes,
            governor: self.clone(),
            released: false,
        })
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
    /// Growth performs real floor/elastic admission and shrink returns the
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
        let plan = self.root.plan();
        let next_total = state
            .scribe_memory_used_bytes
            .checked_add(growth)
            .ok_or_else(accounting_overflow)?;
        let prior_borrow = state
            .scribe_memory_used_bytes
            .saturating_sub(plan.scribe_floor_bytes);
        let added_elastic = next_total.saturating_sub(plan.scribe_floor_bytes) - prior_borrow;
        let next_elastic = state
            .elastic_memory_used_bytes
            .checked_add(added_elastic)
            .ok_or_else(accounting_overflow)?;
        if limit_bytes.is_some_and(|limit| next_total > limit) {
            return Err(BifrostResourceError::Occupied {
                detail: "Scribe resize exceeds the ingress sublimit".to_owned(),
            });
        }
        if next_elastic > plan.elastic_memory_bytes {
            return Err(BifrostResourceError::Occupied {
                detail: "Scribe resize exceeds protected floor plus free elastic memory".to_owned(),
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
        state.scribe_memory_used_bytes = next_total;
        state.elastic_memory_used_bytes = next_elastic;
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
    /// attribution or elastic counter cannot cover the requested shrink.
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
        if state.scribe_memory_used_bytes < shrink
            || state.scribe_category_bytes[category] < shrink
            || shard_total.is_some_and(|total| total < shrink)
        {
            return Err(self
                .root
                .poison_locked(state, "Scribe resize attribution underflow"));
        }
        let plan = self.root.plan();
        let prior_borrow = state
            .scribe_memory_used_bytes
            .saturating_sub(plan.scribe_floor_bytes);
        let next_total = state.scribe_memory_used_bytes - shrink;
        let released_elastic = prior_borrow - next_total.saturating_sub(plan.scribe_floor_bytes);
        if state.elastic_memory_used_bytes < released_elastic {
            return Err(self
                .root
                .poison_locked(state, "Scribe resize elastic underflow"));
        }
        state.scribe_memory_used_bytes = next_total;
        state.elastic_memory_used_bytes -= released_elastic;
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
        let plan = self.root.plan();
        let ingress_limit = plan
            .scribe_floor_bytes
            .checked_add(plan.elastic_memory_bytes)
            .ok_or_else(|| crate::contracts::ScribeError::Internal {
                detail: "validated Scribe role ceiling overflowed".to_owned(),
            })?;
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
        if state.scribe_memory_used_bytes < self.bytes
            || state.scribe_category_bytes[category] < self.bytes
            || shard_bytes.is_some_and(|bytes| bytes < self.bytes)
        {
            return Err(self
                .root
                .poison_locked(&mut state, "Scribe lease release attribution mismatch"));
        }
        let plan = self.root.plan();
        let prior_borrow = state
            .scribe_memory_used_bytes
            .saturating_sub(plan.scribe_floor_bytes);
        let next = state.scribe_memory_used_bytes - self.bytes;
        let next_borrow = next.saturating_sub(plan.scribe_floor_bytes);
        let released_elastic = prior_borrow - next_borrow;
        if state.elastic_memory_used_bytes < released_elastic {
            return Err(self
                .root
                .poison_locked(&mut state, "Scribe lease elastic release underflow"));
        }
        state.scribe_memory_used_bytes = next;
        state.elastic_memory_used_bytes -= released_elastic;
        state.scribe_category_bytes[category] -= self.bytes;
        if let Some(shard) = self.shard {
            let remaining = shard_bytes.unwrap_or_default() - self.bytes;
            if remaining == 0 {
                state.scribe_shard_bytes.remove(&shard);
            } else {
                state.scribe_shard_bytes.insert(shard, remaining);
            }
        }
        state.memory_epoch = state.memory_epoch.wrapping_add(1);
        record_memory_transition("scribe", "released", next);
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

/// Non-cloneable owner of one advertised remote Oracle worker quantum.
#[derive(Debug)]
pub struct OracleWorkerResources {
    /// Exact floor-first root-memory ownership for this remote execution.
    lease: OracleMemoryLease,
    /// Exact bounded `DataFusion` pool nested under the retained root lease.
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
    /// Returns the fixed worker quantum owned until this value is dropped.
    #[must_use]
    pub fn memory_bytes(&self) -> usize {
        self.lease.bytes
    }

    /// Returns the exact bounded pool retained by this worker lease.
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
    /// Returns [`BifrostResourceError::Occupied`] when the Oracle floor plus
    /// free elastic memory cannot cover the fixed slot, or a poison/invalid-plan
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
    /// Returns [`BifrostResourceError::Occupied`] when the Oracle floor plus
    /// free elastic memory cannot cover `bytes`, or a poison/invalid-plan error
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

/// Internal exact Oracle floor/elastic lease shared by the two public shapes.
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
    /// Returns [`BifrostResourceError::Poisoned`] when the retained Oracle or
    /// elastic counters cannot cover this lease.
    fn release(&mut self) -> Result<(), BifrostResourceError> {
        if self.released {
            return Ok(());
        }
        let mut state = self.governor.lock_state()?;
        let plan = self.governor.plan();
        let released_elastic = released_elastic_bytes(
            state.oracle_memory_used_bytes,
            plan.oracle_floor_bytes,
            self.bytes,
        );
        if state.oracle_memory_used_bytes < self.bytes
            || state.elastic_memory_used_bytes < released_elastic
        {
            return Err(self
                .governor
                .poison_locked(&mut state, "Oracle role lease release underflow"));
        }
        state.oracle_memory_used_bytes -= self.bytes;
        state.elastic_memory_used_bytes -= released_elastic;
        state.memory_epoch = state.memory_epoch.wrapping_add(1);
        record_memory_transition("oracle", "released", state.oracle_memory_used_bytes);
        self.released = true;
        drop(state);
        self.governor.inner.memory_changed.notify_waiters();
        Ok(())
    }
}

impl Drop for OracleMemoryLease {
    /// Returns the exact floor-first ownership and poisons on divergence.
    fn drop(&mut self) {
        if let Err(error) = self.release() {
            tracing::error!(%error, "Oracle role resource cleanup failed");
        }
    }
}

/// Query-lifetime Oracle memory, scratch, and adaptive parallelism owner.
#[derive(Debug)]
pub struct OracleQueryResources {
    /// Scheduling class charged by this query owner.
    query_class: QueryClass,
    /// Exact memory this query charges against the shared elastic budget.
    ///
    /// This is the admission charge, not the ceiling the query may reach; see
    /// [`OracleQueryResources::granted_memory_bytes`].
    pub memory_bytes: usize,
    /// Ceiling this query's `DataFusion` pool may grow to.
    ///
    /// Derived once at admission by
    /// [`BifrostResourceGovernor::oracle_memory_grant`] and held for the query's
    /// life. Always at least [`memory_bytes`](Self::memory_bytes) and never above
    /// [`ORACLE_PARTITION_MEMORY_BYTES`].
    pub granted_memory_bytes: usize,
    /// Exact bounded disposable scratch capacity.
    pub scratch_bytes: u64,
    /// Exact slot units retained by this query owner.
    slot_units: u32,
    /// Query-local `DataFusion` target partition count.
    pub target_partitions: usize,
    /// One shared pool used by `DataFusion` and every query-owned Wyrd consumer.
    memory_pool: Arc<dyn MemoryPool>,
    /// Query-local scratch children split from the already admitted envelope.
    nested_scratch_used_bytes: Arc<Mutex<u64>>,
    governor: BifrostResourceGovernor,
    released: bool,
    /// Exact physical Oracle scratch ownership when live roots are registered.
    volume_scratch: Option<ScratchLease>,
}

impl OracleQueryResources {
    /// Builds the tracked first-come, first-served query-local `DataFusion` pool.
    #[must_use]
    pub fn memory_pool(&self) -> Arc<dyn MemoryPool> {
        Arc::clone(&self.memory_pool)
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

    /// Splits exact scratch attribution from the admitted query owner.
    ///
    /// # Errors
    ///
    /// Returns a typed refusal without mutation when sibling scratch children
    /// would exceed the query envelope.
    pub fn try_split_scratch(
        &self,
        bytes: u64,
    ) -> Result<OracleQueryScratchReservation, BifrostResourceError> {
        let mut used =
            self.nested_scratch_used_bytes
                .lock()
                .map_err(|_| BifrostResourceError::Poisoned {
                    detail: "Oracle query scratch attribution lock is poisoned".to_owned(),
                })?;
        let next = used.checked_add(bytes).ok_or_else(accounting_overflow)?;
        if next > self.scratch_bytes {
            return Err(BifrostResourceError::Occupied {
                detail: "Oracle query scratch children exceed the admitted envelope".to_owned(),
            });
        }
        *used = next;
        Ok(OracleQueryScratchReservation {
            bytes,
            used: Arc::clone(&self.nested_scratch_used_bytes),
            governor: self.governor.clone(),
            released: false,
        })
    }

    /// Releases the query envelope only after every nested child is gone.
    ///
    /// # Errors
    ///
    /// Returns a poison error while retaining root capacity when nested memory
    /// or scratch ownership survives, or when root counters diverge.
    fn release(&mut self) -> Result<(), BifrostResourceError> {
        if self.released {
            return Ok(());
        }
        let nested_scratch =
            self.nested_scratch_used_bytes
                .lock()
                .map_err(|_| BifrostResourceError::Poisoned {
                    detail: "Oracle query scratch attribution lock is poisoned".to_owned(),
                })?;
        if *nested_scratch != 0 || self.memory_pool.reserved() != 0 {
            drop(nested_scratch);
            self.governor
                .poison("Oracle query owner outlived a nested resource child");
            return Err(BifrostResourceError::Poisoned {
                detail: "Oracle query nested resource child survived owner release".to_owned(),
            });
        }
        drop(nested_scratch);
        let mut state = self.governor.lock_state()?;
        let class_count = match self.query_class {
            QueryClass::Interactive => state.oracle_interactive_queries,
            QueryClass::Analytical => state.oracle_analytical_queries,
        };
        let plan = self.governor.plan();
        let released_elastic = released_elastic_bytes(
            state.oracle_memory_used_bytes,
            plan.oracle_floor_bytes,
            self.memory_bytes,
        );
        if state.oracle_active_queries == 0
            || class_count == 0
            || state.oracle_query_slot_units < self.slot_units
            || state.oracle_query_memory_used_bytes < self.memory_bytes
            || state.oracle_query_scratch_used_bytes < self.scratch_bytes
            || state.oracle_memory_used_bytes < self.memory_bytes
            || state.elastic_memory_used_bytes < released_elastic
            || state.scratch_used_bytes < self.scratch_bytes
        {
            return Err(self
                .governor
                .poison_locked(&mut state, "Oracle query release underflow"));
        }
        state.elastic_memory_used_bytes -= released_elastic;
        state.oracle_memory_used_bytes -= self.memory_bytes;
        state.scratch_used_bytes -= self.scratch_bytes;
        state.oracle_active_queries -= 1;
        match self.query_class {
            QueryClass::Interactive => state.oracle_interactive_queries -= 1,
            QueryClass::Analytical => state.oracle_analytical_queries -= 1,
        }
        state.oracle_query_slot_units -= self.slot_units;
        state.oracle_query_memory_used_bytes -= self.memory_bytes;
        state.oracle_query_scratch_used_bytes -= self.scratch_bytes;
        state.memory_epoch = state.memory_epoch.wrapping_add(1);
        record_memory_transition("oracle", "released", state.oracle_memory_used_bytes);
        self.volume_scratch.take();
        self.released = true;
        drop(state);
        self.governor.inner.memory_changed.notify_waiters();
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

/// Move-only scratch child split from one already admitted Oracle query.
#[derive(Debug)]
pub struct OracleQueryScratchReservation {
    /// Exact scratch bytes attributed to this child.
    bytes: u64,
    /// Shared query-local attribution counter, never a capacity authority.
    used: Arc<Mutex<u64>>,
    /// Root poisoned when query-local attribution diverges.
    governor: BifrostResourceGovernor,
    /// Whether this child has already returned its attribution.
    released: bool,
}

impl OracleQueryScratchReservation {
    /// Returns the exact query-local scratch attribution.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Returns query-local attribution exactly once.
    ///
    /// # Errors
    ///
    /// Returns a poison error and retains suspect attribution on underflow.
    fn release(&mut self) -> Result<(), BifrostResourceError> {
        if self.released {
            return Ok(());
        }
        let mut used = self
            .used
            .lock()
            .map_err(|_| BifrostResourceError::Poisoned {
                detail: "Oracle query scratch attribution lock is poisoned".to_owned(),
            })?;
        if *used < self.bytes {
            self.governor
                .poison("Oracle query scratch attribution diverged");
            return Err(BifrostResourceError::Poisoned {
                detail: "Oracle query scratch attribution underflow".to_owned(),
            });
        }
        *used -= self.bytes;
        self.released = true;
        Ok(())
    }
}

impl Drop for OracleQueryScratchReservation {
    /// Returns the exact nested scratch attribution on every terminal path.
    fn drop(&mut self) {
        if let Err(error) = self.release() {
            tracing::error!(%error, "Oracle query scratch child cleanup failed");
        }
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

/// Largest `DataFusion` batch size an Oracle query at the full grant receives.
///
/// This is `DataFusion`'s own default. A query holding the whole grant cap has
/// room for the throughput that larger batches buy.
pub const ORACLE_MAX_BATCH_SIZE: usize = 8_192;
/// Smallest `DataFusion` batch size an Oracle query at the floor grant receives.
///
/// Below this, per-batch overhead dominates and the query loses more to task
/// bookkeeping than it saves in memory.
pub const ORACLE_MIN_BATCH_SIZE: usize = 1_024;

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
}

impl OracleSessionShape {
    /// Derives every session knob from one grant and the pinned cut's work.
    ///
    /// Partitions narrow to the work actually available, batch size scales
    /// linearly with the grant between [`ORACLE_MIN_BATCH_SIZE`] and
    /// [`ORACLE_MAX_BATCH_SIZE`], and hash joins are disabled once the grant is
    /// near the floor.
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
        let scaled = ORACLE_MAX_BATCH_SIZE.saturating_mul(granted_memory_bytes)
            / ORACLE_PARTITION_MEMORY_BYTES.max(1);
        let batch_size = scaled.clamp(ORACLE_MIN_BATCH_SIZE, ORACLE_MAX_BATCH_SIZE);
        let prefer_hash_join =
            granted_memory_bytes > ORACLE_PARTITION_WORKING_MEMORY_BYTES.saturating_mul(2);
        Self {
            target_partitions,
            batch_size,
            prefer_hash_join,
        }
    }

    /// Builds the `DataFusion` session configuration this shape describes.
    ///
    /// This is the only place the three knobs reach `DataFusion`, so a caller
    /// cannot apply two of them and silently drop the third.
    #[must_use]
    pub fn session_config(self) -> datafusion::execution::context::SessionConfig {
        let mut config = datafusion::execution::context::SessionConfig::new()
            .with_target_partitions(self.target_partitions)
            .with_batch_size(self.batch_size);
        config.options_mut().optimizer.prefer_hash_join = self.prefer_hash_join;
        Self::apply(&mut config);
        config
    }

    /// Applies the fixed Parquet reader pushdown and indexing options every
    /// Oracle session (leader or follower) must set identically.
    ///
    /// A closed leaf predicate recognized by `OracleTableProvider`'s classifier
    /// only prunes files, row groups, and pages if the `DataFusion` session that
    /// actually opens the Parquet files enables pushdown, reorders filters ahead
    /// of decoding, and consults bloom filters/page indexes. Both the leader's
    /// query-execution session ([`Self::session_config`]) and the follower's
    /// per-request dispatch session (`FollowerSessionFactory::create`) route
    /// through this one function rather than setting the four options inline,
    /// so the two paths cannot silently drift apart.
    pub fn apply(config: &mut datafusion::execution::context::SessionConfig) {
        let parquet_options = &mut config.options_mut().execution.parquet;
        parquet_options.pushdown_filters = true;
        parquet_options.reorder_filters = true;
        parquet_options.bloom_filter_on_read = true;
        parquet_options.enable_page_index = true;
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
#[must_use]
pub(crate) fn bounded_memory_pool(limit_bytes: usize) -> Arc<dyn MemoryPool> {
    let pool: Arc<dyn MemoryPool> = Arc::new(TrackConsumersPool::new(
        FairSpillPool::new(limit_bytes.max(1)),
        NonZeroUsize::new(16).unwrap_or(NonZeroUsize::MIN),
    ));
    #[cfg(any(test, feature = "test-support"))]
    {
        Arc::new(PeakTrackingMemoryPool::new(pool))
    }
    #[cfg(not(any(test, feature = "test-support")))]
    pool
}

#[cfg(any(test, feature = "test-support"))]
static TEST_MEMORY_PEAK_BYTES: AtomicUsize = AtomicUsize::new(0);
/// Named reservation peaks observed by production pool callbacks in serialized tests.
#[cfg(any(test, feature = "test-support"))]
static TEST_MEMORY_CONSUMER_PEAKS: LazyLock<Mutex<std::collections::BTreeMap<String, usize>>> =
    LazyLock::new(|| Mutex::new(std::collections::BTreeMap::new()));

/// Resets the process-wide peak observation used by serialized Forge tests.
///
/// # Panics
///
/// Panics when another test poisoned the shared observation ledger lock.
#[cfg(feature = "test-support")]
pub fn reset_memory_peak_for_test() {
    TEST_MEMORY_PEAK_BYTES.store(0, Ordering::Release);
    TEST_MEMORY_CONSUMER_PEAKS
        .lock()
        .expect("test memory peak ledger lock remains available")
        .clear();
}

/// Returns the largest leased-pool reservation observed since the last reset.
#[cfg(feature = "test-support")]
#[must_use]
pub fn memory_peak_for_test() -> usize {
    TEST_MEMORY_PEAK_BYTES.load(Ordering::Acquire)
}

/// Returns the largest reservation observed for consumer names containing `fragment`.
///
/// # Panics
///
/// Panics when another test poisoned the shared observation ledger lock.
#[cfg(feature = "test-support")]
#[must_use]
pub fn memory_consumer_peak_for_test(fragment: &str) -> usize {
    TEST_MEMORY_CONSUMER_PEAKS
        .lock()
        .expect("test memory peak ledger lock remains available")
        .iter()
        .filter(|(name, _)| name.contains(fragment))
        .map(|(_, peak)| *peak)
        .max()
        .unwrap_or(0)
}

/// Test-only wrapper that records peak reservations without changing admission.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug)]
struct PeakTrackingMemoryPool {
    /// Production finite pool receiving every accounting operation unchanged.
    inner: Arc<dyn MemoryPool>,
}

#[cfg(any(test, feature = "test-support"))]
impl PeakTrackingMemoryPool {
    /// Wraps one production pool for observation only.
    fn new(inner: Arc<dyn MemoryPool>) -> Self {
        Self { inner }
    }

    /// Records the current reservation after a successful growth operation.
    fn observe(&self, reservation: &MemoryReservation) {
        TEST_MEMORY_PEAK_BYTES.fetch_max(self.inner.reserved(), Ordering::AcqRel);
        TEST_MEMORY_CONSUMER_PEAKS
            .lock()
            .expect("test memory peak ledger lock remains available")
            .entry(reservation.consumer().name().to_owned())
            .and_modify(|peak| *peak = (*peak).max(reservation.size()))
            .or_insert_with(|| reservation.size());
    }
}

#[cfg(any(test, feature = "test-support"))]
impl std::fmt::Display for PeakTrackingMemoryPool {
    /// Renders the pool name `DataFusion` reports in resource-exhaustion errors.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("peak_tracking")
    }
}

#[cfg(any(test, feature = "test-support"))]
impl MemoryPool for PeakTrackingMemoryPool {
    /// Returns the stable pool name `DataFusion` attributes reservations to.
    fn name(&self) -> &'static str {
        "peak_tracking"
    }

    /// Delegates consumer registration without changing ordering.
    fn register(&self, consumer: &MemoryConsumer) {
        self.inner.register(consumer);
    }

    /// Delegates consumer removal without changing accounting.
    fn unregister(&self, consumer: &MemoryConsumer) {
        self.inner.unregister(consumer);
    }

    /// Delegates infallible growth and records the resulting peak.
    fn grow(&self, reservation: &MemoryReservation, additional: usize) {
        self.inner.grow(reservation, additional);
        self.observe(reservation);
    }

    /// Delegates release exactly to the production pool.
    fn shrink(&self, reservation: &MemoryReservation, shrink: usize) {
        self.inner.shrink(reservation, shrink);
    }

    /// Delegates fallible growth and records only successful reservations.
    fn try_grow(
        &self,
        reservation: &MemoryReservation,
        additional: usize,
    ) -> datafusion::error::Result<()> {
        self.inner.try_grow(reservation, additional)?;
        self.observe(reservation);
        Ok(())
    }

    /// Returns the production pool's live reservation.
    fn reserved(&self) -> usize {
        self.inner.reserved()
    }

    /// Returns the production pool's unchanged finite-memory contract.
    fn memory_limit(&self) -> MemoryLimit {
        self.inner.memory_limit()
    }
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

/// Computes the elastic bytes a role release returns to the shared pool.
///
/// Elastic borrowing is a function of how far a role's *total* sits above its
/// protected floor, never of what an individual lease borrowed when it was
/// admitted. Replaying a per-lease borrow at release is correct only when
/// leases release in exact reverse acquisition order; concurrent Oracle queries
/// and Forge tasks do not, and each out-of-order release strands the difference
/// permanently because these counters are never re-derived from ownership.
///
/// Worked example with a 256 MiB floor: lease A takes 256 MiB and borrows 0,
/// lease B then takes 256 MiB and borrows 256 MiB. Releasing A first returns
/// A's recorded 0, leaving the pool holding 256 MiB that the remaining total no
/// longer justifies. The next reconciliation poisons accounting and terminates
/// the process.
///
/// `released` must already be known not to exceed `role_used`; callers check
/// that before poisoning on underflow. The subtraction cannot underflow because
/// the post-release borrow is monotone in the role total.
fn released_elastic_bytes(role_used: usize, floor: usize, released: usize) -> usize {
    let prior_borrow = role_used.saturating_sub(floor);
    let next_total = role_used.saturating_sub(released);
    prior_borrow - next_total.saturating_sub(floor)
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
    use datafusion::execution::memory_pool::MemoryConsumer;
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;

    /// Every Oracle session owner must set all four Parquet reader pushdown
    /// and indexing options to exactly `true`; a closed leaf predicate only
    /// prunes files, row groups, and pages when the reader is configured to
    /// use it.
    #[test]
    fn oracle_reader_session_options_contract() {
        let mut config = datafusion::execution::context::SessionConfig::new();
        OracleSessionShape::apply(&mut config);
        let parquet_options = &config.options().execution.parquet;
        assert!(parquet_options.pushdown_filters);
        assert!(parquet_options.reorder_filters);
        assert!(parquet_options.bloom_filter_on_read);
        assert!(parquet_options.enable_page_index);
    }

    fn policy(roles: &[BifrostRole]) -> BifrostResourcePolicy {
        let roles: BTreeSet<BifrostRole> = roles.iter().copied().collect();
        BifrostResourcePolicy {
            forge_compaction_memory_limit_bytes: forge_budget_for_test(&roles),
            roles,
            memory_limit_bytes: None,
            unmanaged_reserve_bytes: None,
            scratch_limit_bytes: None,
            effective_cpu: None,
            oracle_query_slot_limit: None,
            scratch_root: PathBuf::new(),
            volume_roots: None,
        }
    }

    fn snapshot(memory: usize) -> SystemResourceSnapshot {
        SystemResourceSnapshot {
            memory_limit_bytes: memory,
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
            resources.plan().unmanaged_reserve_bytes
        );

        let aggregate = resources.plan().unmanaged_reserve_bytes;
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
    /// Admission charges one slot-unit quantum rather than a whole grant cap, so
    /// a fixture box that once held exactly one worker now holds several. Tests
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
        let plan = oracle.snapshot().expect("planned budget").plan;
        let budget = plan.oracle_floor_bytes + plan.elastic_memory_bytes;
        let reserved = ORACLE_PARTITION_WORKING_MEMORY_BYTES * spare;
        let mut held = Vec::new();
        let mut charged = oracle
            .snapshot()
            .expect("current charge")
            .oracle_memory_used_bytes;
        while charged + ORACLE_PARTITION_WORKING_MEMORY_BYTES + reserved <= budget {
            held.push(
                oracle
                    .try_acquire_worker(OracleWorkerClass::Interactive)
                    .expect("a worker fitting inside the remaining budget is admitted"),
            );
            charged += ORACLE_PARTITION_WORKING_MEMORY_BYTES;
        }
        held
    }

    /// Builds one exact interactive query quantum for root-ledger tests.
    fn interactive_query(local_ratio: f64) -> OracleResourceRequest {
        OracleResourceRequest::for_class(QueryClass::Interactive, local_ratio)
    }

    /// Builds one exact analytical query quantum for root-ledger tests.
    fn analytical_query(local_ratio: f64) -> OracleResourceRequest {
        OracleResourceRequest::for_class(QueryClass::Analytical, local_ratio)
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
            768 * MIB,
            "the role capability projects the sole root plan"
        );
        assert!(roles.oracle().is_some());
        assert_eq!(runtime.sources().memory, ResourceSource::Injected);
    }

    /// Invalid complete observations fail before any role handle is returned.
    #[test]
    fn runtime_resources_reject_invalid_snapshot_before_role_activation() {
        let memory_error = BifrostRuntimeResources::from_snapshot(
            snapshot(512 * MIB - 1),
            policy(&[BifrostRole::Oracle]),
        );
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

    /// Full mixed composition rejects memory below Forge's executable floor.
    #[test]
    fn mixed_forge_requires_one_rewrite_working_set() {
        let error = BifrostRuntimeResources::from_snapshot(
            snapshot(832 * MIB - 1),
            policy(&[BifrostRole::Scribe, BifrostRole::Oracle, BifrostRole::Forge]),
        )
        .expect_err("mixed Forge must retain one executable rewrite grant");
        assert!(matches!(error, BifrostResourceError::InvalidPlan { .. }));
        BifrostRuntimeResources::from_snapshot(
            snapshot(832 * MIB),
            policy(&[BifrostRole::Scribe, BifrostRole::Oracle, BifrostRole::Forge]),
        )
        .expect("exact mixed Forge memory floor must compose");
    }

    /// Every supported and internal topology accepts its exact constitutional floor.
    ///
    /// # Panics
    ///
    /// Panics when an exact topology cannot be constructed.
    #[test]
    fn resource_plan_accepts_exact_topology_boundaries_and_rejects_one_byte_less() {
        let cases = [
            (
                &[BifrostRole::Scribe, BifrostRole::Oracle, BifrostRole::Forge][..],
                832 * MIB,
            ),
            (&[BifrostRole::Forge][..], 320 * MIB),
            (&[BifrostRole::Scribe, BifrostRole::Forge][..], 576 * MIB),
            (&[BifrostRole::Oracle, BifrostRole::Forge][..], 576 * MIB),
            (&[BifrostRole::Scribe, BifrostRole::Oracle][..], 768 * MIB),
        ];
        for (roles, minimum) in cases {
            BifrostRuntimeResources::from_snapshot(snapshot(minimum), policy(roles))
                .expect("the exact constitutional topology floor must compose")
                .compose_roles()
                .expect("exact topology roles must issue from one root");
            assert!(matches!(
                BifrostRuntimeResources::from_snapshot(snapshot(minimum - 1), policy(roles)),
                Err(BifrostResourceError::InvalidPlan { .. })
            ));
        }
    }

    /// A floor-only Scribe budget in mixed topology admits a generation and producer delta.
    ///
    /// This models the production handoff: the immutable generation remains
    /// charged while the Parquet producer acquires only the complement to its
    /// 256 MiB owner. Together they consume, but never exceed, the same role
    /// floor, and dropping both owners returns root attribution to baseline.
    ///
    /// # Panics
    ///
    /// Panics if exact-floor composition, ingress admission, category transfer,
    /// producer admission, attribution inspection, or release reconciliation
    /// violates the Scribe ownership contract.
    #[test]
    fn scribe_exact_floor_admits_generation_and_transfer() {
        let roles = BifrostRuntimeResources::composed_for_test(
            832 * MIB,
            512 * MIB as u64,
            [BifrostRole::Scribe, BifrostRole::Oracle, BifrostRole::Forge],
        );
        let scribe = roles.scribe().expect("Scribe capability");
        assert_eq!(scribe.governor.plan().scribe_floor_bytes, 256 * MIB);
        assert_eq!(scribe.governor.plan().elastic_memory_bytes, 0);
        assert_eq!(scribe.ingress_limit_bytes(), 256 * MIB);

        // The exact-floor property is what this test exists to prove: the
        // retained generation plus the producer's incremental workspace must
        // sum to the role floor precisely. Derive the generation from the
        // projection rather than hard-coding both sides, so a change to the
        // workspace formula keeps the scenario exact instead of silently
        // overshooting the floor and turning this into a refusal test.
        let producer_delta = crate::scribe::memory::parquet_candidate_incremental_bytes(72 * MIB)
            .expect("candidate workspace projection");
        let generation_bytes = (256 * MIB) - producer_delta;
        let mut generation = scribe
            .try_reserve_ingress(ScribeMemoryCategory::Active, generation_bytes)
            .expect("representative generation must fit the exact Scribe floor");
        generation
            .transfer_category(ScribeMemoryCategory::Immutable)
            .expect("generation ownership must transfer to immutable");
        let producer = scribe
            .try_reserve_maintenance(ScribeMemoryCategory::Persistence, producer_delta)
            .expect("producer delta must complete the exact Scribe floor");
        // Deriving the generation makes the sum equal the floor by
        // construction, so assert the consequence that is not tautological:
        // the floor is genuinely full and one further byte is refused.
        assert!(
            matches!(
                scribe.try_reserve_ingress(ScribeMemoryCategory::Active, 1),
                Err(crate::contracts::ScribeError::IngestBusy { .. })
            ),
            "generation plus producer workspace must exactly exhaust the Scribe floor"
        );

        let occupied = scribe.snapshot().expect("occupied Scribe snapshot");
        assert_eq!(
            occupied.scribe_memory_used_bytes,
            generation_bytes + producer_delta
        );
        assert_eq!(occupied.elastic_memory_used_bytes, 0);
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

    /// Mixed-topology ingress resize retains the full floor without elastic memory.
    ///
    /// # Panics
    ///
    /// Panics if a floor-sized resize is refused, a byte beyond the floor is
    /// admitted, or releasing the resized owner fails to restore baseline.
    #[test]
    fn scribe_ingress_resize_uses_exact_floor_without_elastic() {
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
            .resize_ingress(256 * MIB)
            .expect("full floor resize must remain admissible");
        assert!(ingress.resize_ingress(256 * MIB + 1).is_err());
        assert_eq!(ingress.bytes(), 256 * MIB);
        assert_eq!(
            scribe
                .snapshot()
                .expect("floor-sized ingress snapshot")
                .elastic_memory_used_bytes,
            0
        );

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

    /// Oracle worker and metadata leases spend their floor before shared elastic memory.
    ///
    /// # Panics
    ///
    /// Panics when deterministic resource acquisition or inspection fails.
    #[test]
    fn resource_plan_oracle_worker_and_metadata_leases_share_floor_first_accounting() {
        let roles = BifrostRuntimeResources::composed_for_test(
            576 * MIB,
            512 * MIB as u64,
            [BifrostRole::Oracle, BifrostRole::Forge],
        );
        let oracle = roles.oracle().expect("Oracle capability");
        let worker = oracle
            .try_acquire_worker(OracleWorkerClass::Interactive)
            .expect("one worker spends only the Oracle floor");
        assert_eq!(
            worker.memory_bytes(),
            ORACLE_PARTITION_WORKING_MEMORY_BYTES,
            "a worker charges the slot-unit admission quantum, not the grant cap"
        );
        assert_eq!(
            oracle
                .snapshot()
                .expect("worker snapshot")
                .elastic_memory_used_bytes,
            0
        );
        let filled = saturate_oracle_workers(&oracle, 0);
        assert!(
            oracle.metadata().try_acquire_footer_slot().is_err(),
            "a footer slot is refused once workers hold the whole Oracle budget"
        );
        drop(filled);
        drop(worker);
        let footer = oracle
            .metadata()
            .try_acquire_footer_slot()
            .expect("footer slot fits the released Oracle floor");
        assert_eq!(footer.memory_bytes(), ORACLE_METADATA_MEMORY_BYTES);
        assert_eq!(
            oracle
                .snapshot()
                .expect("footer snapshot")
                .elastic_memory_used_bytes,
            0
        );
        drop(footer);
        assert_eq!(
            oracle
                .snapshot()
                .expect("released snapshot")
                .oracle_memory_used_bytes,
            0
        );
    }

    /// Concurrent Oracle owners atomically admit one floor quantum without rollback drift.
    ///
    /// # Panics
    ///
    /// Panics when thread coordination or the exact-floor fixture fails.
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
            .oracle_memory_used_bytes;
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
                .oracle_memory_used_bytes,
            charged_before + ORACLE_PARTITION_WORKING_MEMORY_BYTES,
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

    /// Aliased roots share one exact device ceiling and provisional WAL rolls back.
    ///
    /// # Panics
    ///
    /// Panics when the deterministic volume fixture cannot be constructed.
    #[test]
    fn bifrost_volume_governor_groups_aliases_and_preserves_exact_capacity() {
        let temp = tempfile::tempdir().expect("temporary volume root");
        let wal = temp.path().join("wal");
        let stage_root = temp.path().join("scribe-stage");
        let scribe = temp.path().join("scribe-output-scratch");
        let oracle = temp.path().join("oracle");
        for path in [&wal, &stage_root, &scribe, &oracle] {
            fs::create_dir(path).expect("registered volume root");
        }
        fs::write(wal.join("retained.wal"), [0_u8; 16]).expect("retained WAL fixture");
        let health = BifrostResourceHealth::default();
        let governor = BifrostVolumeGovernor::register(
            BifrostVolumeRoots {
                wal,
                scribe_stage: stage_root,
                scribe_output_scratch: scribe,
                oracle_scratch: oracle,
            },
            80,
            health.clone(),
        )
        .expect("same-device roots register once");
        let capabilities = governor.capabilities();
        {
            let provisional = capabilities
                .wal
                .try_reserve_growth(32)
                .expect("provisional WAL growth");
            assert!(capabilities.oracle.try_acquire(33).is_err());
            drop(provisional);
        }
        let scratch = capabilities
            .oracle
            .try_acquire(64)
            .expect("retained WAL plus exact-limit scratch succeeds");
        assert!(capabilities.oracle.try_acquire(1).is_err());
        drop(scratch);
        let growth = capabilities
            .wal
            .try_reserve_growth(64)
            .expect("exact-limit WAL growth");
        growth.commit().expect("durable WAL commit");
        assert!(capabilities.scribe_output.try_acquire(1).is_err());
        assert_eq!(health.reason(), None);
    }

    /// Concurrent same-device WAL and scratch grants admit exactly one boundary owner.
    ///
    /// # Panics
    ///
    /// Panics when thread coordination or the deterministic volume fixture fails.
    #[test]
    fn bifrost_volume_governor_serializes_same_device_wal_scratch_race() {
        let temp = tempfile::tempdir().expect("temporary volume root");
        let wal = temp.path().join("wal");
        let stage_root = temp.path().join("scribe-stage");
        let scribe = temp.path().join("scribe-output-scratch");
        let oracle = temp.path().join("oracle");
        for path in [&wal, &stage_root, &scribe, &oracle] {
            fs::create_dir(path).expect("registered volume root");
        }
        let governor = BifrostVolumeGovernor::register(
            BifrostVolumeRoots {
                wal,
                scribe_stage: stage_root,
                scribe_output_scratch: scribe,
                oracle_scratch: oracle,
            },
            64,
            BifrostResourceHealth::default(),
        )
        .expect("same-device roots");
        let start = Arc::new(std::sync::Barrier::new(3));
        let finish = Arc::new(std::sync::Barrier::new(3));
        let (sender, receiver) = std::sync::mpsc::channel();
        let mut joins = Vec::new();
        for wal_request in [true, false] {
            let governor = governor.clone();
            let start = Arc::clone(&start);
            let finish = Arc::clone(&finish);
            let sender = sender.clone();
            joins.push(std::thread::spawn(move || {
                start.wait();
                let owner = if wal_request {
                    governor
                        .capabilities()
                        .wal
                        .try_reserve_growth(64)
                        .map(|owner| Box::new(owner) as Box<dyn std::any::Any>)
                } else {
                    governor
                        .capabilities()
                        .oracle
                        .try_acquire(64)
                        .map(|owner| Box::new(owner) as Box<dyn std::any::Any>)
                };
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
        finish.wait();
        for join in joins {
            join.join().expect("volume race thread");
        }
        assert_eq!(governor.health.reason(), None);
    }

    /// Scribe scratch cancellation retains ownership without touching foreign siblings.
    ///
    /// # Panics
    ///
    /// Panics when deterministic namespace setup or cleanup fails.
    #[test]
    fn bifrost_volume_governor_scribe_namespace_is_exactly_scoped() {
        let temp = tempfile::tempdir().expect("temporary volume root");
        let wal = temp.path().join("wal");
        let stage_root = wal.join("scribe-stage");
        let scribe = wal.join("scribe-output-scratch");
        let oracle = wal.join("oracle");
        for path in [&wal, &stage_root, &scribe, &oracle] {
            fs::create_dir_all(path).expect("registered volume root");
        }
        let retained_wal = wal.join("retained.wal");
        let peer = scribe.join("peer-owned");
        let stale = scribe.join("scribe-claim-old-claim-0");
        fs::write(&retained_wal, [1_u8]).expect("retained WAL");
        fs::create_dir(&peer).expect("peer directory");
        fs::create_dir(&stale).expect("stale runtime directory");

        let health = BifrostResourceHealth::default();
        let governor = BifrostVolumeGovernor::register(
            BifrostVolumeRoots {
                wal,
                scribe_stage: stage_root,
                scribe_output_scratch: scribe.clone(),
                oracle_scratch: oracle,
            },
            1024,
            health.clone(),
        )
        .expect("registered roots reconcile process-owned scratch");
        assert!(!stale.exists());
        assert!(peer.exists());
        assert!(retained_wal.exists());

        let scratch = governor
            .capabilities()
            .scribe_output
            .create_scribe_claim("stream_1", "claim_9", 32)
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
        assert!(owned.exists());
        assert_eq!(health.reason(), Some(BifrostResourcePoisonReason::Volume));
        assert!(peer.exists());
        assert!(retained_wal.exists());
    }

    /// Final exact-prefix cleanup failure retains charge and poisons once.
    ///
    /// # Panics
    ///
    /// Panics when the deterministic injected failure does not fail closed.
    #[test]
    fn bifrost_volume_governor_cleanup_failure_retains_charge_and_poisons() {
        let temp = tempfile::tempdir().expect("temporary volume root");
        let wal = temp.path().join("wal");
        let stage_root = wal.join("scribe-stage");
        let scribe = wal.join("scribe-output-scratch");
        let oracle = wal.join("oracle");
        for path in [&wal, &stage_root, &scribe, &oracle] {
            fs::create_dir_all(path).expect("registered volume root");
        }
        let health = BifrostResourceHealth::default();
        let governor = BifrostVolumeGovernor::register(
            BifrostVolumeRoots {
                wal,
                scribe_stage: stage_root,
                scribe_output_scratch: scribe,
                oracle_scratch: oracle,
            },
            1024,
            health.clone(),
        )
        .expect("registered roots");
        let mut scratch = governor
            .capabilities()
            .scribe_output
            .create_scribe_claim("stream", "claim_4", 32)
            .expect("claim scratch");
        let attempts = std::cell::Cell::new(0);
        let error = scratch
            .cleanup_owned_prefix_with(|| {
                attempts.set(attempts.get() + 1);
                Err(std::io::Error::other("injected unlink failure"))
            })
            .expect_err("exhausted cleanup must fail closed");
        assert!(matches!(error, BifrostResourceError::Poisoned { .. }));
        assert_eq!(attempts.get(), 4);
        assert_eq!(health.reason(), Some(BifrostResourcePoisonReason::Volume));
        let root = governor
            .roots
            .get(&BifrostVolumeClass::ScribeOutput)
            .expect("Scribe root");
        assert_eq!(
            governor
                .devices
                .lock()
                .expect("device state")
                .get(&root.device)
                .expect("Scribe device")
                .scratch_bytes
                .get(&BifrostVolumeClass::ScribeOutput)
                .copied()
                .unwrap_or_default(),
            32
        );
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

    /// Closed resource telemetry covers plans, grants, refusals, releases, and volume classes.
    ///
    /// # Panics
    ///
    /// Panics when the deterministic owner lifecycle or metric assertions fail.
    #[test]
    fn resource_plan_metrics_cover_closed_memory_and_volume_lifecycle() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        metrics::with_local_recorder(&recorder, || {
            let roles = BifrostRuntimeResources::composed_for_test(
                576 * MIB,
                512 * MIB as u64,
                [BifrostRole::Oracle, BifrostRole::Forge],
            );
            let oracle = roles.oracle().expect("Oracle capability");
            let filled = saturate_oracle_workers(&oracle, 1);
            let owner = oracle
                .try_acquire_worker(OracleWorkerClass::Interactive)
                .expect("Oracle grant");
            assert!(
                oracle
                    .try_acquire_worker(OracleWorkerClass::Interactive)
                    .is_err(),
                "a saturated budget refuses and records the refusal"
            );
            drop(owner);
            drop(filled);

            let temp = tempfile::tempdir().expect("temporary volume root");
            let wal = temp.path().join("wal");
            let stage_root = temp.path().join("scribe-stage");
            let scribe = temp.path().join("scribe-output-scratch");
            let oracle_root = temp.path().join("oracle");
            for path in [&wal, &stage_root, &scribe, &oracle_root] {
                fs::create_dir(path).expect("registered volume root");
            }
            let volumes = BifrostVolumeGovernor::register(
                BifrostVolumeRoots {
                    wal,
                    scribe_stage: stage_root,
                    scribe_output_scratch: scribe,
                    oracle_scratch: oracle_root,
                },
                64,
                BifrostResourceHealth::default(),
            )
            .expect("volume plan");
            let capability = volumes.capabilities().oracle;
            let scratch = capability.try_acquire(64).expect("volume grant");
            assert!(capability.try_acquire(1).is_err());
            drop(scratch);
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
            "volume_class=\"oracle\"",
            "bifrost_resource_current_bytes",
        ] {
            assert!(
                metric_exists(fragment),
                "missing metric fragment {fragment}"
            );
        }
    }

    /// Roots on distinct filesystem devices retain independent exact ceilings.
    ///
    /// # Panics
    ///
    /// Panics when an available distinct-device fixture violates its boundaries.
    #[cfg(unix)]
    #[test]
    fn bifrost_volume_governor_keeps_distinct_devices_independent() {
        let disk = tempfile::tempdir().expect("disk-backed temporary root");
        let Ok(memory) = tempfile::tempdir_in("/dev/shm") else {
            return;
        };
        let wal = disk.path().join("wal");
        let stage_root = disk.path().join("scribe-stage");
        let scribe = disk.path().join("scribe-output-scratch");
        let oracle = memory.path().join("oracle");
        for path in [&wal, &stage_root, &scribe, &oracle] {
            fs::create_dir(path).expect("registered volume root");
        }
        if fs::metadata(&scribe).expect("Scribe metadata").dev()
            == fs::metadata(&oracle).expect("Oracle metadata").dev()
            || filesystem_available_bytes(&oracle)
                .map_or(true, |available| available < MIN_SCRATCH_FREE_BYTES + 64)
        {
            return;
        }
        let governor = BifrostVolumeGovernor::register(
            BifrostVolumeRoots {
                wal,
                scribe_stage: stage_root,
                scribe_output_scratch: scribe,
                oracle_scratch: oracle,
            },
            64,
            BifrostResourceHealth::default(),
        )
        .expect("distinct roots register");
        let capabilities = governor.capabilities();
        let scribe_lease = capabilities
            .scribe_output
            .try_acquire(64)
            .expect("Scribe output consumes its device boundary");
        let oracle_lease = capabilities
            .oracle
            .try_acquire(64)
            .expect("Oracle independently consumes its device boundary");
        assert!(capabilities.scribe_output.try_acquire(1).is_err());
        assert!(capabilities.oracle.try_acquire(1).is_err());
        drop((scribe_lease, oracle_lease));
    }

    /// Enabled roles alone receive protected floors and elastic arithmetic is exact.
    #[test]
    fn resource_plan_reserves_only_enabled_role_floors() {
        let gib = 1024 * MIB;
        let cases = [
            (&[BifrostRole::Oracle][..], 0, 256 * MIB, 512 * MIB),
            (&[BifrostRole::Scribe][..], 256 * MIB, 0, 512 * MIB),
            // Forge protects no floor, but its budget is reserved out of the
            // same managed memory, so the elastic remainder is what the
            // observation's budget leaves rather than the whole of it.
            (
                &[BifrostRole::Forge][..],
                0,
                0,
                768 * MIB - FORGE_TEST_BUDGET_BYTES,
            ),
            (
                &[BifrostRole::Scribe, BifrostRole::Oracle][..],
                256 * MIB,
                256 * MIB,
                256 * MIB,
            ),
        ];
        for (roles, scribe, oracle, elastic) in cases {
            let runtime = BifrostRuntimeResources::from_snapshot(snapshot(gib), policy(roles))
                .expect("resource plan must fit");
            let plan = runtime.plan();
            assert_eq!(plan.managed_memory_bytes, 768 * MIB);
            assert_eq!(plan.scribe_floor_bytes, scribe);
            assert_eq!(plan.oracle_floor_bytes, oracle);
            assert_eq!(plan.elastic_memory_bytes, elastic);
        }
    }

    /// Memory and scratch ownership is one atomic Oracle grant and exact release.
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
        // Scratch, not memory, is the dimension that saturates this fixture: a
        // query charges one slot-unit quantum of memory but reserves a full
        // grant cap of consumable disk.
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
                scribe_memory_used_bytes: 0,
                oracle_memory_used_bytes: 0,
                elastic_memory_used_bytes: 0,
                scratch_used_bytes: 0,
                oracle_active_queries: 0,
                oracle_interactive_queries: 0,
                oracle_analytical_queries: 0,
                oracle_query_slot_units: 0,
                oracle_query_memory_used_bytes: 0,
                oracle_query_scratch_used_bytes: 0,
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
        injected.memory_limit_bytes = 1024 * MIB;
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
        policy.memory_limit_bytes = Some(768 * MIB);
        policy.effective_cpu = Some(2);
        let runtime = BifrostRuntimeResources::from_snapshot(snapshot(1024 * MIB), policy)
            .expect("reducing overrides must be accepted");
        assert_eq!(runtime.plan().memory_limit_bytes, 768 * MIB);
        assert_eq!(runtime.plan().effective_cpu, 2);
        assert_eq!(runtime.sources().memory, ResourceSource::Override);
        assert_eq!(runtime.sources().cpu, ResourceSource::Override);
    }

    /// Overrides cap one global plan and never create per-role silos.
    #[test]
    fn resource_detector_applies_absolute_overrides_without_role_silos() {
        let mut policy = policy(&[BifrostRole::Scribe, BifrostRole::Oracle]);
        policy.memory_limit_bytes = Some(768 * MIB);
        policy.scratch_limit_bytes = Some(512 * MIB as u64);
        let runtime = BifrostRuntimeResources::from_snapshot(snapshot(1024 * MIB), policy)
            .expect("combined minimum must fit");
        assert_eq!(runtime.plan().managed_memory_bytes, 512 * MIB);
        assert_eq!(runtime.plan().elastic_memory_bytes, 0);
        assert_eq!(runtime.plan().scratch_limit_bytes, 512 * MIB as u64);
        assert_eq!(runtime.sources().scratch, ResourceSource::Override);
    }

    /// Minimum process and filesystem reserves fail closed before activation.
    #[test]
    fn resource_plan_enforces_minimum_viable_process_and_disk_floors() {
        assert!(
            BifrostRuntimeResources::from_snapshot(
                snapshot(512 * MIB - 1),
                policy(&[BifrostRole::Oracle]),
            )
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

    /// Enabled floors that exceed managed memory fail before any lease exists.
    #[test]
    fn resource_plan_rejects_floors_above_managed_memory_before_activation() {
        let error = BifrostRuntimeResources::from_snapshot(
            snapshot(768 * MIB - 1),
            policy(&[BifrostRole::Scribe, BifrostRole::Oracle]),
        )
        .expect_err("combined floors must not be weakened");
        assert!(matches!(error, BifrostResourceError::InvalidPlan { .. }));
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
            query.memory_bytes, ORACLE_PARTITION_WORKING_MEMORY_BYTES,
            "an interactive query charges one slot-unit quantum against the budget"
        );
        let pool = query.memory_pool();
        let reservation = MemoryConsumer::new("oracle-test").register(&pool);
        assert!(
            query.granted_memory_bytes > query.memory_bytes,
            "an otherwise idle node grants far more ceiling than the query charges"
        );
        reservation
            .try_grow(query.granted_memory_bytes)
            .expect("the pool is sized by the grant, not by the admission charge");
        assert!(reservation.try_grow(1).is_err());
        reservation.shrink(query.granted_memory_bytes);
        assert_eq!(pool.reserved(), 0);
        drop(query);
        assert!(!roles.snapshot().expect("snapshot").oracle_query_active);
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
        policy.memory_limit_bytes = Some(1024 * MIB);
        match BifrostRuntimeResources::detect(policy.clone()) {
            Ok(runtime) => {
                assert_eq!(runtime.plan().memory_limit_bytes, 1024 * MIB);
                assert_eq!(runtime.sources().memory, ResourceSource::Override);
                assert_eq!(runtime.sources().scratch, ResourceSource::Filesystem);
                let detected = detect_snapshot(&policy.scratch_root, policy.memory_limit_bytes)
                    .expect("detection succeeded once already");
                let replayed = BifrostRuntimeResources::from_snapshot(detected, policy)
                    .expect("the injected path accepts the detected observation");
                let live_plan = runtime.plan();
                let replayed_plan = replayed.plan();
                assert_eq!(
                    (
                        replayed_plan.memory_limit_bytes,
                        replayed_plan.effective_cpu,
                        replayed_plan.unmanaged_reserve_bytes,
                        replayed_plan.managed_memory_bytes,
                        replayed_plan.scribe_floor_bytes,
                        replayed_plan.oracle_floor_bytes,
                        replayed_plan.elastic_memory_bytes,
                    ),
                    (
                        live_plan.memory_limit_bytes,
                        live_plan.effective_cpu,
                        live_plan.unmanaged_reserve_bytes,
                        live_plan.managed_memory_bytes,
                        live_plan.scribe_floor_bytes,
                        live_plan.oracle_floor_bytes,
                        live_plan.elastic_memory_bytes,
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
        // The root's charge is what the request reserved, not the grant the
        // plan sizes the query's pool at: the grant is a ceiling derived from
        // the plan's own elastic memory and moves when any other role's budget
        // does, so equating the two would make this a plan-arithmetic test.
        assert_eq!(occupied.oracle_memory_used_bytes, request.memory_bytes);
        assert!(lease.granted_memory_bytes >= request.memory_bytes);
        drop(lease);
        assert_eq!(
            oracle
                .snapshot()
                .expect("released")
                .elastic_memory_used_bytes,
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

    /// Oracle worker classes acquire exactly one or two quanta atomically.
    #[test]
    fn oracle_worker_classes_use_exact_root_quanta() {
        let roles = BifrostRuntimeResources::composed_for_test(
            768 * MIB,
            512 * MIB as u64,
            [BifrostRole::Oracle],
        );
        let oracle = roles.oracle().expect("Oracle capability");
        let plan = oracle.snapshot().expect("planned budget").plan;
        let budget = plan.oracle_floor_bytes + plan.elastic_memory_bytes;
        let analytical = oracle
            .try_acquire_worker(OracleWorkerClass::Analytical)
            .expect("two-quanta worker");
        assert_eq!(
            analytical.memory_bytes(),
            2 * ORACLE_PARTITION_WORKING_MEMORY_BYTES,
            "an analytical worker charges two slot-unit quanta"
        );
        // The budget no longer holds a single worker, so exhaustion has to be
        // driven rather than assumed. Every acquisition below the budget must
        // succeed and the first one above it must be refused.
        let mut held = vec![analytical];
        let mut charged = 2 * ORACLE_PARTITION_WORKING_MEMORY_BYTES;
        while charged + ORACLE_PARTITION_WORKING_MEMORY_BYTES <= budget {
            held.push(
                oracle
                    .try_acquire_worker(OracleWorkerClass::Interactive)
                    .expect("a worker fitting inside the remaining budget is admitted"),
            );
            charged += ORACLE_PARTITION_WORKING_MEMORY_BYTES;
        }
        assert_eq!(
            oracle
                .snapshot()
                .expect("saturated")
                .oracle_memory_used_bytes,
            charged
        );
        assert!(
            oracle
                .try_acquire_worker(OracleWorkerClass::Interactive)
                .is_err(),
            "a worker that does not fit the remaining budget is refused"
        );
        drop(held);
        assert_eq!(
            oracle
                .snapshot()
                .expect("released")
                .oracle_memory_used_bytes,
            0
        );
    }

    /// Query memory and scratch children split admitted ownership without root charge.
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
        let scratch = query
            .try_split_scratch(64 * MIB as u64)
            .expect("nested scratch child");
        assert_eq!(memory.bytes(), 32 * MIB);
        assert_eq!(scratch.bytes(), 64 * MIB as u64);
        assert_eq!(oracle.snapshot().expect("nested snapshot"), root_snapshot);
        drop(memory);
        drop(scratch);
        drop(query);
        assert!(!oracle.snapshot().expect("released").oracle_query_active);
    }

    /// Exact interactive queries overlap until one aggregate dimension is full.
    #[test]
    fn oracle_exact_queries_overlap_until_memory_or_scratch_exhaustion() {
        let roles = BifrostRuntimeResources::composed_for_test(
            1024 * MIB,
            512 * MIB as u64,
            [BifrostRole::Oracle],
        );
        let oracle = roles.oracle().expect("Oracle capability");
        let first = oracle
            .try_acquire_query(interactive_query(0.0))
            .expect("first exact query");
        let second = oracle
            .try_acquire_query(interactive_query(1.0))
            .expect("second exact query");
        let occupied = oracle.snapshot().expect("aggregate snapshot");
        assert_eq!(occupied.oracle_active_queries, 2);
        assert_eq!(
            occupied.oracle_query_memory_used_bytes,
            2 * ORACLE_PARTITION_WORKING_MEMORY_BYTES,
            "two interactive queries charge two slot-unit quanta between them"
        );
        // Scratch is still reserved at the grant cap because it is consumed disk
        // rather than a ceiling, so scratch — not memory — is the dimension that
        // saturates first here.
        assert_eq!(occupied.oracle_query_scratch_used_bytes, 512 * MIB as u64);
        assert!(oracle.try_acquire_query(interactive_query(0.5)).is_err());
        assert_eq!(oracle.snapshot().expect("atomic refusal"), occupied);
        drop(first);
        let replacement = oracle
            .try_acquire_query(interactive_query(0.5))
            .expect("release restores exact eligibility");
        drop((second, replacement));
        let released = oracle.snapshot().expect("released aggregate snapshot");
        assert_eq!(released.oracle_active_queries, 0);
        assert_eq!(released.oracle_memory_used_bytes, 0);
        assert_eq!(released.scratch_used_bytes, 0);
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
                scratch_bytes: (2 * ORACLE_PARTITION_MEMORY_BYTES) as u64,
                ..interactive_query(0.0)
            },
            OracleResourceRequest {
                slot_units: 2,
                ..interactive_query(0.0)
            },
            OracleResourceRequest {
                query_class: QueryClass::Analytical,
                memory_bytes: ORACLE_PARTITION_MEMORY_BYTES,
                scratch_bytes: (2 * ORACLE_PARTITION_MEMORY_BYTES) as u64,
                slot_units: 2,
                local_ratio: 0.0,
            },
            OracleResourceRequest {
                query_class: QueryClass::Analytical,
                memory_bytes: 2 * ORACLE_PARTITION_MEMORY_BYTES,
                scratch_bytes: ORACLE_PARTITION_MEMORY_BYTES as u64,
                slot_units: 2,
                local_ratio: 0.0,
            },
            OracleResourceRequest {
                query_class: QueryClass::Analytical,
                memory_bytes: 2 * ORACLE_PARTITION_MEMORY_BYTES,
                scratch_bytes: (2 * ORACLE_PARTITION_MEMORY_BYTES) as u64,
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
        let budget = plan.oracle_floor_bytes + plan.elastic_memory_bytes;

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
        let budget = plan.oracle_floor_bytes + plan.elastic_memory_bytes;
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
        assert!(
            floor.batch_size < full.batch_size,
            "a floor grant must hold less per batch than a full grant"
        );
        assert_eq!(full.batch_size, ORACLE_MAX_BATCH_SIZE);
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

        let floor_config = floor.session_config();
        assert!(
            !floor_config.options().optimizer.prefer_hash_join,
            "a floor-grant session disables the one operator that cannot spill"
        );
        assert_eq!(floor_config.target_partitions(), floor.target_partitions);
        assert_eq!(floor_config.batch_size(), floor.batch_size);
    }

    /// Analytical admission preserves one complete interactive query quantum.
    #[test]
    fn analytical_capacity_preserves_one_interactive_quantum() {
        // Scratch and the slot ceiling are both deliberately generous so the
        // memory dimension is the one that binds; the protected reserve this
        // test covers is a memory rule, and it cannot be observed through a
        // refusal that slot capacity issued first.
        let mut policy = policy(&[BifrostRole::Oracle]);
        policy.oracle_query_slot_limit = Some(1024);
        let mut probe = snapshot(1280 * MIB);
        probe.scratch_capacity_bytes = 64 * 1024 * MIB as u64;
        probe.scratch_available_bytes = 64 * 1024 * MIB as u64;
        let roles = BifrostRuntimeResources::from_snapshot(probe, policy)
            .expect("analytical reserve plan")
            .compose_roles()
            .expect("analytical reserve composition");
        let oracle = roles.oracle().expect("Oracle capability");
        let mut analytical = vec![
            oracle
                .try_acquire_query(analytical_query(0.0))
                .expect("analytical query below protected reserve"),
        ];
        while let Ok(query) = oracle.try_acquire_query(analytical_query(0.0)) {
            analytical.push(query);
        }
        let occupied = oracle.snapshot().expect("analytical owners at the reserve");
        let budget = occupied.plan.oracle_floor_bytes + occupied.plan.elastic_memory_bytes;
        let remaining = budget
            .checked_sub(occupied.oracle_memory_used_bytes)
            .expect("ordinary Oracle memory remainder");
        assert!(
            remaining >= ORACLE_PARTITION_WORKING_MEMORY_BYTES,
            "analytical admission stopped while one interactive quantum still fits"
        );
        assert!(
            remaining < 3 * ORACLE_PARTITION_WORKING_MEMORY_BYTES,
            "analytical admission consumed everything above the protected reserve, \
             leaving less than one further analytical charge plus that reserve"
        );

        let refused = oracle.try_acquire_query(analytical_query(0.0));
        assert!(matches!(
            refused,
            Err(BifrostResourceError::Occupied { .. })
        ));
        assert_eq!(oracle.snapshot().expect("reserve refusal"), occupied);
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

    /// Plans one node from an injected observation and an explicit policy.
    fn plan_for(
        memory_limit_bytes: usize,
        roles: &[BifrostRole],
        override_bytes: Option<usize>,
    ) -> Result<ResourcePlan, BifrostResourceError> {
        let scratch_limit_bytes = 1024 * MIB as u64;
        let scratch_available_bytes = scratch_limit_bytes + MIN_SCRATCH_FREE_BYTES;
        BifrostRuntimeResources::from_snapshot(
            SystemResourceSnapshot {
                memory_limit_bytes,
                effective_cpu: 4,
                scratch_capacity_bytes: scratch_available_bytes,
                scratch_available_bytes,
                memory_source: ResourceSource::Injected,
                cpu_source: ResourceSource::Injected,
            },
            BifrostResourcePolicy {
                roles: roles.iter().copied().collect(),
                memory_limit_bytes: None,
                unmanaged_reserve_bytes: None,
                scratch_limit_bytes: Some(scratch_limit_bytes),
                effective_cpu: None,
                oracle_query_slot_limit: None,
                forge_compaction_memory_limit_bytes: override_bytes,
                scratch_root: PathBuf::new(),
                volume_roots: None,
            },
        )
        .map(|resources| resources.plan())
    }

    /// The Forge compaction budget is derived once and never crosses a floor.
    ///
    /// This is the whole of Forge's root memory ownership: one immutable figure
    /// on the plan, reserved at planning time, that the worker's admission queue
    /// charges running plans against. There is no live Forge root lease, so if
    /// this arithmetic is wrong nothing later corrects it — which is why the
    /// dedicated, co-located, absent, and rounding cases are pinned here on
    /// their owner rather than inferred from a worker test. Overrides and
    /// refusals are pinned by
    /// [`forge_compaction_budget_refuses_rather_than_clamping`].
    ///
    /// # Panics
    ///
    /// Panics when any case selects a different budget, leaves different
    /// elastic memory, or weakens a protected role floor.
    #[test]
    fn forge_compaction_budget_preserves_role_floors() {
        // Dedicated Forge: no protected floor competes, so the whole managed
        // pool less the derived budget stays elastic.
        let memory = 4096 * MIB;
        let dedicated = plan_for(memory, &[BifrostRole::Forge], None).expect("dedicated Forge");
        assert_eq!(dedicated.scribe_floor_bytes, 0);
        assert_eq!(dedicated.oracle_floor_bytes, 0);
        let expected_default = memory * 4 / 5;
        assert_eq!(
            dedicated.forge_compaction_memory_limit_bytes, expected_default,
            "the default budget is four fifths of the resolved memory limit"
        );
        assert_eq!(
            dedicated.elastic_memory_bytes,
            dedicated.managed_memory_bytes - expected_default
        );

        // Co-located `All`: the same formula, but both protected floors are
        // still deducted before anything is elastic.
        let colocated = plan_for(
            memory,
            &[BifrostRole::Scribe, BifrostRole::Oracle, BifrostRole::Forge],
            None,
        )
        .expect("co-located All");
        assert_eq!(colocated.scribe_floor_bytes, ROLE_MEMORY_FLOOR_BYTES);
        assert_eq!(colocated.oracle_floor_bytes, ROLE_MEMORY_FLOOR_BYTES);
        assert_eq!(
            colocated.forge_compaction_memory_limit_bytes, expected_default,
            "the budget formula does not change with co-location"
        );
        let safe = colocated.managed_memory_bytes
            - colocated.scribe_floor_bytes
            - colocated.oracle_floor_bytes;
        assert_eq!(colocated.elastic_memory_bytes, safe - expected_default);

        // Forge absent: zero budget, and today's elastic result is preserved.
        let without = plan_for(memory, &[BifrostRole::Scribe, BifrostRole::Oracle], None)
            .expect("Forge-absent node");
        assert_eq!(without.forge_compaction_memory_limit_bytes, 0);
        assert_eq!(
            without.elastic_memory_bytes,
            without.managed_memory_bytes - without.scribe_floor_bytes - without.oracle_floor_bytes
        );

        // Rounding floors rather than rounds: a limit that is not a multiple of
        // five loses the remainder to elastic instead of over-committing.
        let odd = plan_for(memory + 3, &[BifrostRole::Forge], None).expect("odd memory limit");
        assert_eq!(
            odd.forge_compaction_memory_limit_bytes,
            (memory + 3) * 4 / 5
        );
    }

    /// An override wins outright, and a budget that does not fit refuses.
    ///
    /// The refusals are the point: silently shrinking an over-large budget
    /// would let an operator believe Forge was admitted memory it never had,
    /// and accepting a zero one would admit a worker that can run no plan.
    ///
    /// # Panics
    ///
    /// Panics when an override is not honoured exactly, or when a zero,
    /// above-safe, or overflowing budget is clamped instead of refused.
    #[test]
    fn forge_compaction_budget_refuses_rather_than_clamping() {
        let memory = 4096 * MIB;
        let expected_default = memory * 4 / 5;
        let colocated = plan_for(
            memory,
            &[BifrostRole::Scribe, BifrostRole::Oracle, BifrostRole::Forge],
            None,
        )
        .expect("co-located All");

        // An explicit positive override wins outright, in either direction.
        for override_bytes in [64 * MIB, expected_default + MIB] {
            let overridden = plan_for(memory, &[BifrostRole::Forge], Some(override_bytes))
                .expect("an override inside the safe budget is accepted");
            assert_eq!(
                overridden.forge_compaction_memory_limit_bytes,
                override_bytes
            );
            assert_eq!(
                overridden.elastic_memory_bytes,
                overridden.managed_memory_bytes - override_bytes
            );
        }

        // Zero and above-safe both refuse; neither is clamped into range.
        assert!(
            plan_for(memory, &[BifrostRole::Forge], Some(0)).is_err(),
            "a zero Forge budget admits no plan and must refuse boot"
        );
        let colocated_safe = colocated.managed_memory_bytes
            - colocated.scribe_floor_bytes
            - colocated.oracle_floor_bytes;
        assert!(
            plan_for(
                memory,
                &[BifrostRole::Scribe, BifrostRole::Oracle, BifrostRole::Forge],
                Some(colocated_safe + 1),
            )
            .is_err(),
            "a budget past the protected floors must refuse boot, never clamp"
        );

        // The overflow guard is checked arithmetic, not a wrapping multiply.
        assert!(
            plan_for(usize::MAX, &[BifrostRole::Forge], None).is_err(),
            "a memory limit whose four-fifths derivation overflows must refuse"
        );
    }

    /// Every role retains multiple exact owners in the same checked root ledger.
    #[test]
    fn role_leases_share_one_root_without_crossing_floors_or_elastic() {
        let roles = BifrostRuntimeResources::composed_for_test(
            1536 * MIB,
            1024 * MIB as u64,
            [BifrostRole::Scribe, BifrostRole::Oracle, BifrostRole::Forge],
        );
        let oracle = roles.oracle().expect("Oracle capability");
        let scribe = roles.scribe().expect("Scribe capability");
        let queries = [
            oracle
                .try_acquire_query(interactive_query(0.0))
                .expect("query one"),
            oracle
                .try_acquire_query(interactive_query(1.0))
                .expect("query two"),
        ];
        let scribe_leases = [
            scribe
                .try_acquire_memory(ScribeMemoryRequest {
                    bytes: 64 * MIB,
                    category: ScribeMemoryCategory::Raw,
                    shard: Some(0),
                })
                .expect("Scribe lease one"),
            scribe
                .try_acquire_memory(ScribeMemoryRequest {
                    bytes: 64 * MIB,
                    category: ScribeMemoryCategory::Queued,
                    shard: Some(1),
                })
                .expect("Scribe lease two"),
        ];
        let snapshot = roles.snapshot().expect("shared-root snapshot");
        assert!(
            snapshot.scribe_memory_used_bytes + snapshot.oracle_memory_used_bytes
                <= snapshot.plan.managed_memory_bytes
        );
        // Stated as the floor-first invariant rather than a fixed number: elastic
        // memory holds exactly what each role borrowed beyond its own floor. A
        // literal here would only re-encode one fixture's arithmetic and would go
        // stale the next time an admission charge changes.
        let expected_elastic = snapshot
            .scribe_memory_used_bytes
            .saturating_sub(snapshot.plan.scribe_floor_bytes)
            + snapshot
                .oracle_memory_used_bytes
                .saturating_sub(snapshot.plan.oracle_floor_bytes);
        assert_eq!(snapshot.elastic_memory_used_bytes, expected_elastic);
        drop((queries, scribe_leases));
        let released = roles.snapshot().expect("shared-root release");
        assert_eq!(released.scribe_memory_used_bytes, 0);
        assert_eq!(released.oracle_memory_used_bytes, 0);
        assert_eq!(released.scratch_used_bytes, 0);
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
        let scratch = explicit.try_split_scratch(1).expect("scratch child");
        drop(scratch);
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
        let child = owner.try_split_scratch(1).expect("surviving child");
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
        assert_eq!(released.elastic_memory_used_bytes, 0);
        assert_eq!(released.scratch_used_bytes, 0);
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

    /// Scribe's floor remains outside every Oracle and Forge elastic lease.
    /// Releasing role leases out of acquisition order must leave elastic
    /// borrowing exactly equal to what the remaining role total justifies.
    ///
    /// Elastic borrow is a property of a role's total against its floor, not of
    /// any one lease. Charging each release the borrow it recorded at admission
    /// is only correct under strict LIFO release, which concurrent queries never
    /// guarantee. Each out-of-order release used to strand the difference, and
    /// because these counters are never re-derived, the drift accumulated until
    /// reconciliation poisoned accounting and terminated the process.
    #[test]
    fn out_of_order_query_release_leaves_no_stranded_elastic_memory() {
        let roles = BifrostRuntimeResources::composed_for_test(
            4096 * MIB,
            2048 * MIB as u64,
            [BifrostRole::Scribe, BifrostRole::Oracle, BifrostRole::Forge],
        );
        let oracle = roles.oracle().expect("Oracle capability");
        let baseline = roles
            .snapshot()
            .expect("baseline snapshot")
            .elastic_memory_used_bytes;

        // The first query fits under the Oracle floor and borrows nothing; the
        // second is entirely elastic. Releasing the first one first is the case
        // that used to strand its successor's borrow.
        // The interactive quantum is exactly one role floor, so the first query
        // borrows nothing and the second borrows a full floor's worth. That is
        // the pairing that strands memory when the first one releases first.
        let first = oracle
            .try_acquire_query(interactive_query(0.0))
            .expect("first query is admitted entirely under the Oracle floor");
        let second = oracle
            .try_acquire_query(interactive_query(0.0))
            .expect("second query is admitted entirely from elastic memory");
        drop(first);
        drop(second);

        let snapshot = roles.snapshot().expect("post-release snapshot");
        assert_eq!(
            snapshot.oracle_memory_used_bytes, 0,
            "both queries released their role memory"
        );
        assert_eq!(
            snapshot.elastic_memory_used_bytes, baseline,
            "out-of-order release stranded elastic memory"
        );
        // Reconciliation is the check that fails closed in production, so assert
        // the accounting it validates is actually intact rather than only the
        // counter this test set out to fix.
        roles
            .snapshot()
            .expect("resource accounting reconciles after out-of-order release");
    }

    #[test]
    fn scribe_floor_survives_oracle_elastic_pressure() {
        let roles = BifrostRuntimeResources::composed_for_test(
            1024 * MIB,
            512 * MIB as u64,
            [BifrostRole::Scribe, BifrostRole::Oracle, BifrostRole::Forge],
        );
        let plan = roles.plan();
        assert_eq!(plan.scribe_floor_bytes, ROLE_MEMORY_FLOOR_BYTES);
        let scribe = roles.scribe().expect("Scribe capability");
        let oracle = roles.oracle().expect("Oracle capability");
        let scribe_owner = scribe
            .try_acquire_memory(ScribeMemoryRequest {
                bytes: 300 * MIB,
                category: crate::scribe::memory::MemoryCategory::Active,
                shard: None,
            })
            .expect("Scribe uses its floor and borrows elastic memory");
        let with_scribe = scribe.snapshot().expect("Scribe ownership snapshot");
        assert_eq!(with_scribe.scribe_memory_used_bytes, 300 * MIB);
        assert_eq!(with_scribe.elastic_memory_used_bytes, 44 * MIB);
        let query = oracle
            .try_acquire_query(interactive_query(0.0))
            .expect("Oracle owns only its floor and shared elastic memory");
        assert_eq!(
            query.memory_bytes, ORACLE_PARTITION_WORKING_MEMORY_BYTES,
            "an interactive query charges one slot-unit quantum against the budget"
        );
        assert_eq!(roles.plan().scribe_floor_bytes, ROLE_MEMORY_FLOOR_BYTES);
        drop(query);
        drop(scribe_owner);
        let snapshot = roles.snapshot().expect("released resource snapshot");
        assert_eq!(snapshot.elastic_memory_used_bytes, 0);
        assert_eq!(snapshot.scratch_used_bytes, 0);
    }
}
