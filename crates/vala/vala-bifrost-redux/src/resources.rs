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
    GreedyMemoryPool, MemoryConsumer, MemoryPool, MemoryReservation, TrackConsumersPool,
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
/// Minimum elastic memory required for one executable Forge rewrite.
pub const FORGE_MEMORY_FLOOR_BYTES: usize = 64 * MIB;
/// Filesystem free space that disposable query spill never consumes.
pub const MIN_SCRATCH_FREE_BYTES: u64 = 256 * MIB as u64;
/// Memory represented by one Oracle execution partition.
pub const ORACLE_PARTITION_MEMORY_BYTES: usize = 256 * MIB;
/// Working memory one `DataFusion` execution partition needs to make progress.
///
/// This is deliberately far smaller than [`ORACLE_PARTITION_MEMORY_BYTES`], which
/// sizes a whole query's memory envelope. Dividing a query envelope by the
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

/// Derives advertised Oracle worker slots from the same root lease quantum.
///
/// # Errors
///
/// Returns an invalid-plan error when checked Oracle capacity arithmetic or
/// conversion cannot produce a positive platform-sized slot count.
pub fn oracle_worker_slots(plan: ResourcePlan) -> Result<usize, BifrostResourceError> {
    let budget = plan
        .oracle_floor_bytes
        .checked_add(plan.elastic_memory_bytes)
        .ok_or_else(accounting_overflow)?;
    Ok((budget / ORACLE_PARTITION_MEMORY_BYTES)
        .min(plan.effective_cpu)
        .max(1))
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
    /// Process-owned Scribe output scratch namespace.
    pub scribe_output_scratch: PathBuf,
    /// Forge attempt scratch root.
    pub forge_scratch: PathBuf,
    /// Oracle query scratch root.
    pub oracle_scratch: PathBuf,
}

/// Closed volume purpose used for bounded telemetry and diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BifrostVolumeClass {
    /// Durable write-ahead log occupancy.
    Wal,
    /// Disposable Scribe persistence output.
    ScribeOutput,
    /// Disposable Forge rewrite data.
    Forge,
    /// Disposable Oracle query spill.
    Oracle,
}

impl BifrostVolumeClass {
    /// Returns the closed telemetry label for this physical-volume purpose.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Wal => "wal",
            Self::ScribeOutput => "scribe_output",
            Self::Forge => "forge",
            Self::Oracle => "oracle",
        }
    }

    /// Returns the closed role label responsible for this volume purpose.
    const fn role(self) -> &'static str {
        match self {
            Self::Wal | Self::ScribeOutput => "scribe",
            Self::Forge => "forge",
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
    /// Durable WAL bytes reconciled or committed on this device.
    durable_wal_bytes: u64,
    /// Provisional WAL growth admitted before mutation completes.
    provisional_wal_bytes: u64,
    /// Live disposable scratch leases.
    scratch_bytes: BTreeMap<BifrostVolumeClass, u64>,
    /// Whether accounting on this device remains trustworthy.
    poisoned: bool,
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
            (
                BifrostVolumeClass::ScribeOutput,
                roots.scribe_output_scratch,
            ),
            (BifrostVolumeClass::Forge, roots.forge_scratch),
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
            .durable_wal_bytes = retained;
        let governor = Self {
            roots: Arc::new(registered),
            devices: Arc::new(Mutex::new(devices)),
            configured_limit_bytes,
            health,
        };
        for class in [
            BifrostVolumeClass::Wal,
            BifrostVolumeClass::ScribeOutput,
            BifrostVolumeClass::Forge,
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
            scribe_output: ScratchVolume {
                governor: self.clone(),
                class: BifrostVolumeClass::ScribeOutput,
            },
            forge: ScratchVolume {
                governor: self.clone(),
                class: BifrostVolumeClass::Forge,
            },
            oracle: ScratchVolume {
                governor: self.clone(),
                class: BifrostVolumeClass::Oracle,
            },
        }
    }

    /// Returns exact class ownership for deterministic WAL and scratch tests.
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
            state.durable_wal_bytes,
            state.provisional_wal_bytes,
            state.scratch_bytes.get(&class).copied().unwrap_or_default(),
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
    fn acquire(
        &self,
        class: BifrostVolumeClass,
        bytes: u64,
        wal: bool,
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
        let scratch_used = state
            .scratch_bytes
            .values()
            .try_fold(0_u64, |total, bytes| total.checked_add(*bytes))
            .ok_or_else(accounting_overflow)?;
        let used = state
            .durable_wal_bytes
            .checked_add(state.provisional_wal_bytes)
            .and_then(|value| value.checked_add(scratch_used))
            .ok_or_else(accounting_overflow)?;
        let next = used.checked_add(bytes).ok_or_else(accounting_overflow)?;
        if next > self.configured_limit_bytes || next > available {
            let current = if wal {
                state
                    .durable_wal_bytes
                    .saturating_add(state.provisional_wal_bytes)
            } else {
                state.scratch_bytes.get(&class).copied().unwrap_or_default()
            };
            record_volume_transition(class, "refused", current);
            return Err(BifrostResourceError::Occupied {
                detail: "physical-volume request exceeds configured or live-free capacity"
                    .to_owned(),
            });
        }
        if wal {
            state.provisional_wal_bytes += bytes;
        } else {
            *state.scratch_bytes.entry(class).or_default() += bytes;
        }
        let current = if wal {
            state
                .durable_wal_bytes
                .saturating_add(state.provisional_wal_bytes)
        } else {
            state.scratch_bytes.get(&class).copied().unwrap_or_default()
        };
        record_volume_transition(class, "acquired", current);
        Ok(VolumeLease {
            governor: self.clone(),
            device: root.device,
            bytes,
            class,
            wal,
            retained: false,
        })
    }
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
        if name
            .to_str()
            .is_some_and(|name| name.starts_with("scribe-runtime-"))
        {
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
    /// Scribe output-scratch capability.
    pub scribe_output: ScratchVolume,
    /// Forge scratch capability.
    pub forge: ScratchVolume,
    /// Oracle scratch capability.
    pub oracle: ScratchVolume,
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
        let root = self
            .governor
            .roots
            .get(&BifrostVolumeClass::Wal)
            .ok_or_else(accounting_overflow)?;
        let mut devices =
            self.governor
                .devices
                .lock()
                .map_err(|_| BifrostResourceError::Poisoned {
                    detail: "volume state lock is poisoned".to_owned(),
                })?;
        let state = devices
            .get_mut(&root.device)
            .ok_or_else(accounting_overflow)?;
        if state.durable_wal_bytes < bytes {
            state.poisoned = true;
            self.governor
                .health
                .poison(BifrostResourcePoisonReason::Volume);
            return Err(BifrostResourceError::Poisoned {
                detail: "durable WAL retirement underflow".to_owned(),
            });
        }
        state.durable_wal_bytes -= bytes;
        record_volume_transition(BifrostVolumeClass::Wal, "released", state.durable_wal_bytes);
        Ok(())
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

    /// Creates one generation-owned Scribe output directory after exact admission.
    ///
    /// The returned owner removes only the directory it creates. Its name is
    /// rooted beneath the registered Scribe namespace and carries the stream
    /// and generation identity needed for restart reconciliation.
    ///
    /// # Errors
    ///
    /// Returns a typed plan error for a non-Scribe capability, a capacity
    /// refusal from the shared device governor, or an unavailable error when
    /// the exact owned directory cannot be created.
    pub fn create_scribe_generation(
        &self,
        stream: &str,
        generation: u64,
        bytes: u64,
    ) -> Result<ScribeGenerationScratch, BifrostResourceError> {
        if self.class != BifrostVolumeClass::ScribeOutput {
            return Err(BifrostResourceError::InvalidPlan {
                detail: "Scribe generation scratch requires the Scribe output capability"
                    .to_owned(),
            });
        }
        let component = safe_scratch_component(stream)?;
        let lease = self.try_acquire(bytes)?;
        let root = self
            .governor
            .roots
            .get(&self.class)
            .ok_or_else(accounting_overflow)?;
        let suffix = SCRATCH_NAMESPACE_SEQUENCE.fetch_add(1, AtomicOrdering::Relaxed);
        let path = root
            .path
            .join(format!("scribe-runtime-{component}-{generation}-{suffix}"));
        fs::create_dir(&path).map_err(|error| BifrostResourceError::Unavailable {
            detail: format!("cannot create Scribe generation scratch: {error}"),
        })?;
        Ok(ScribeGenerationScratch {
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

/// Generation-owned Scribe output directory and its exact physical charge.
#[derive(Debug)]
pub struct ScribeGenerationScratch {
    /// Exact directory created for this generation.
    path: PathBuf,
    /// Registered parent used to prove cleanup containment and fsync completion.
    namespace_root: PathBuf,
    /// Charge released only after confirmed directory removal and parent fsync.
    lease: Option<ScratchLease>,
    /// Shared fail-stop signal poisoned after the final cleanup failure.
    health: BifrostResourceHealth,
}

impl ScribeGenerationScratch {
    /// Returns the exact directory available to the generation writer.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Removes the exact generation directory and releases its physical charge.
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
            detail: "Scribe generation scratch cleanup exhausted bounded retries".to_owned(),
        })
    }
}

impl Drop for ScribeGenerationScratch {
    /// Applies the same exact-prefix cleanup on cancellation and unwind.
    fn drop(&mut self) {
        if let Err(error) = self.cleanup_owned_prefix() {
            tracing::error!(%error, path = %self.path.display(), "Scribe scratch cleanup failed");
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
    let mut final_error = None;
    for delay in SCRATCH_CLEANUP_BACKOFFS {
        std::thread::sleep(delay);
        match cleanup() {
            Ok(()) => return Ok(()),
            Err(error) => final_error = Some(error),
        }
    }
    Err(final_error.unwrap_or_else(|| std::io::Error::other("scratch cleanup was not attempted")))
}

/// Deletes one proven child directory and fsyncs its registered parent.
fn remove_exact_scratch_prefix(root: &Path, owned: &Path) -> std::io::Result<()> {
    if owned.parent() != Some(root)
        || !owned
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("scribe-runtime-"))
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
        let mut lease = self.lease.take().ok_or_else(accounting_overflow)?;
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
        if state.provisional_wal_bytes < lease.bytes {
            state.poisoned = true;
            lease
                .governor
                .health
                .poison(BifrostResourcePoisonReason::Volume);
            return Err(BifrostResourceError::Poisoned {
                detail: "provisional WAL growth diverged before commit".to_owned(),
            });
        }
        state.provisional_wal_bytes -= lease.bytes;
        state.durable_wal_bytes = state
            .durable_wal_bytes
            .checked_add(lease.bytes)
            .ok_or_else(accounting_overflow)?;
        lease.retained = true;
        record_volume_transition(
            BifrostVolumeClass::Wal,
            "committed",
            state.durable_wal_bytes,
        );
        Ok(())
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
    /// Whether this is provisional WAL rather than disposable scratch.
    wal: bool,
    /// Durable WAL or failed-cleanup ownership retained after this handle drops.
    retained: bool,
}

impl Drop for VolumeLease {
    /// Rolls back provisional WAL or releases exact disposable scratch.
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
        let counter = if self.wal {
            &mut state.provisional_wal_bytes
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
    /// Memory protected for the server and non-Bifrost work.
    pub unmanaged_reserve_bytes: usize,
    /// Total memory governed by Bifrost.
    pub managed_memory_bytes: usize,
    /// Protected Scribe floor, or zero when Scribe is inactive.
    pub scribe_floor_bytes: usize,
    /// Protected Oracle floor, or zero when Oracle is inactive.
    pub oracle_floor_bytes: usize,
    /// Protected Forge floor, or zero when Forge is inactive.
    pub forge_floor_bytes: usize,
    /// Memory available after every enabled role's protected floor.
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
    /// Total live Forge ownership, including its protected floor.
    pub forge_memory_used_bytes: usize,
    /// Elastic memory held by Oracle or Forge.
    pub elastic_memory_used_bytes: usize,
    /// Disposable scratch held by Oracle or Forge.
    pub scratch_used_bytes: u64,
    /// Concurrent Forge input-reader permits held by active rewrites.
    pub forge_reader_permits_used: usize,
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
#[derive(Debug, Clone)]
pub(crate) struct ScribeMemoryRequest {
    /// Exact bytes that become owned on successful admission.
    pub bytes: usize,
    /// Lifecycle category charged by this owner.
    pub category: ScribeMemoryCategory,
    /// Optional bounded shard attribution.
    pub shard: Option<usize>,
    /// Optional durable artifact-generation attribution.
    pub generation: Option<crate::scribe::seal_key::ScribeArtifactIdentity>,
}

/// Closed Scribe attribution exported only for production-equivalent tests.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceAttributionSnapshot {
    /// Exact category totals indexed by [`ScribeMemoryCategory`] discriminant.
    pub category_bytes: [usize; crate::scribe::memory::MEMORY_CATEGORY_COUNT],
    /// Exact shard totals for shards that currently own bytes.
    pub shard_bytes: BTreeMap<usize, usize>,
    /// Number of live leases that intentionally omit generation attribution.
    pub omitted_generation_count: usize,
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

/// Closed remote Oracle worker sizes admitted atomically by the target root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OracleWorkerClass {
    /// One 256 MiB quantum for ordinary fragment execution.
    Interactive,
    /// Two 256 MiB quanta for analytical fragment execution.
    Analytical,
}

impl OracleWorkerClass {
    /// Returns the exact atomic root-memory request for this worker class.
    fn memory_bytes(self) -> usize {
        match self {
            Self::Interactive => ORACLE_PARTITION_MEMORY_BYTES,
            Self::Analytical => 2 * ORACLE_PARTITION_MEMORY_BYTES,
        }
    }
}

#[derive(Debug, Default)]
struct ResourceState {
    scribe_memory_used_bytes: usize,
    oracle_memory_used_bytes: usize,
    forge_memory_used_bytes: usize,
    elastic_memory_used_bytes: usize,
    scratch_used_bytes: u64,
    forge_reader_permits_used: usize,
    oracle_active_queries: u32,
    oracle_interactive_queries: u32,
    oracle_analytical_queries: u32,
    oracle_query_slot_units: u32,
    oracle_query_memory_used_bytes: usize,
    oracle_query_scratch_used_bytes: u64,
    scribe_category_bytes: [usize; crate::scribe::memory::MEMORY_CATEGORY_COUNT],
    scribe_shard_bytes: BTreeMap<usize, usize>,
    scribe_generation_bytes: usize,
    scribe_omitted_generation_bytes: usize,
    scribe_omitted_generation_count: usize,
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
                roles: roles.into_iter().collect(),
                memory_limit_bytes: None,
                unmanaged_reserve_bytes: None,
                scratch_limit_bytes: Some(scratch_limit_bytes),
                effective_cpu: None,
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
            forge: self
                .governor
                .is_enabled(BifrostRole::Forge)
                .then(|| ForgeResources {
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
    forge: Option<ForgeResources>,
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
                generation: None,
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
                .saturating_add(state.oracle_memory_used_bytes)
                .saturating_add(state.forge_memory_used_bytes),
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
            omitted_generation_count: state.scribe_omitted_generation_count,
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
                    generation: None,
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
            generation: None,
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
        let memory_pool = bounded_memory_pool(lease.bytes);
        Ok(OracleWorkerResources { lease, memory_pool })
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

/// Exact bounded Forge rewrite demand derived from validated durable estimates.
///
/// A request is only ever formed from estimates that already passed planner
/// capacity validation. Configuration ceilings bound a request; they are never
/// themselves the requested demand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForgeRewriteRequest {
    /// Authoritative persisted per-term envelope.
    pub envelope: vala_sql::row_types::forge_tasks::ForgeTaskEnvelope,
    /// Exact rewrite working-memory demand in bytes.
    pub memory_bytes: usize,
    /// Exact disposable spill demand in bytes.
    pub scratch_bytes: u64,
    /// Concurrent input readers reserved before any source object is opened.
    pub reader_permits: u16,
}

/// Closed result of finalizing one exact Forge rewrite lease.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeResourceReleaseResult {
    /// Every leased counter returned to the root governor exactly once.
    Released,
    /// Accounting was made fail-closed and the leased counters were retained.
    Poisoned,
}

/// Sole issuer of Forge rewrite leases against the shared process root.
#[derive(Debug, Clone)]
pub struct ForgeResources {
    governor: BifrostResourceGovernor,
    /// Physical Forge scratch authority registered during live boot.
    volumes: Option<BifrostVolumeGovernor>,
}

impl ForgeResources {
    /// Returns the shared process resource-health lifecycle signal.
    #[must_use]
    pub fn health(&self) -> BifrostResourceHealth {
        self.governor.inner.health.clone()
    }
    /// Atomically acquires the exact requested rewrite memory and scratch.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Occupied`] without partial ownership
    /// when either counter does not fit currently free capacity.
    pub fn try_acquire_rewrite(
        &self,
        request: ForgeRewriteRequest,
    ) -> Result<ForgeRewriteResources, BifrostResourceError> {
        let mut resources = self.governor.try_acquire_forge(
            request.memory_bytes,
            request.scratch_bytes,
            request.reader_permits,
        )?;
        if let Some(volumes) = &self.volumes {
            let capabilities = volumes.capabilities();
            resources.volume_scratch = Some(capabilities.forge.try_acquire(request.scratch_bytes)?);
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
    /// Exact roles activated by the checked policy stage.
    ///
    /// Forge carries no protected memory floor, so the plan alone cannot report
    /// whether the role is enabled. Role composition reads this set instead of
    /// re-deriving activation from floor bytes.
    roles: BTreeSet<BifrostRole>,
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
) -> Result<(usize, usize, usize, usize), BifrostResourceError> {
    let scribe = usize::from(roles.contains(&BifrostRole::Scribe)) * ROLE_MEMORY_FLOOR_BYTES;
    let oracle = usize::from(roles.contains(&BifrostRole::Oracle)) * ROLE_MEMORY_FLOOR_BYTES;
    let forge = usize::from(roles.contains(&BifrostRole::Forge)) * FORGE_MEMORY_FLOOR_BYTES;
    let protected = scribe
        .checked_add(oracle)
        .and_then(|bytes| bytes.checked_add(forge))
        .ok_or_else(accounting_overflow)?;
    Ok((scribe, oracle, forge, protected))
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
        let (scribe_floor_bytes, oracle_floor_bytes, forge_floor_bytes, protected) =
            role_memory_floors(&policy.roles)?;
        let elastic_memory_bytes = managed_memory_bytes.checked_sub(protected).ok_or_else(|| {
            BifrostResourceError::InvalidPlan {
                detail: format!("managed memory {managed_memory_bytes} cannot cover enabled role floors {protected}"),
            }
        })?;
        let configured_scratch = policy
            .scratch_limit_bytes
            .map_or(snapshot.scratch_capacity_bytes, |limit| {
                limit.min(snapshot.scratch_capacity_bytes)
            });
        let available_after_floor = snapshot
            .scratch_available_bytes
            .checked_sub(MIN_SCRATCH_FREE_BYTES)
            .ok_or_else(|| BifrostResourceError::InvalidPlan {
                detail: format!("scratch availability {} cannot preserve filesystem floor {MIN_SCRATCH_FREE_BYTES}", snapshot.scratch_available_bytes),
            })?;
        let scratch_limit_bytes = configured_scratch.min(available_after_floor);
        if policy.roles.contains(&BifrostRole::Oracle) && scratch_limit_bytes == 0 {
            return Err(BifrostResourceError::InvalidPlan {
                detail: "Oracle requires positive disposable scratch after the filesystem floor"
                    .to_owned(),
            });
        }
        drop(policy.scratch_root);
        let root = Self {
            inner: Arc::new(ResourceGovernorInner {
                plan: ResourcePlan {
                    memory_limit_bytes,
                    effective_cpu,
                    unmanaged_reserve_bytes,
                    managed_memory_bytes,
                    scribe_floor_bytes,
                    oracle_floor_bytes,
                    forge_floor_bytes,
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
                roles: policy.roles,
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

    /// Reports whether the checked policy stage activated `role`.
    #[must_use]
    pub(crate) fn is_enabled(&self, role: BifrostRole) -> bool {
        self.inner.roles.contains(&role)
    }

    /// Publishes immutable closed-role memory and scratch plan gauges.
    fn record_plan_metrics(&self) {
        let plan = self.plan();
        let planned = [
            ("unmanaged", "memory", plan.unmanaged_reserve_bytes),
            ("scribe", "memory", plan.scribe_floor_bytes),
            ("oracle", "memory", plan.oracle_floor_bytes),
            ("forge", "memory", plan.forge_floor_bytes),
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
            forge_memory_used_bytes: state.forge_memory_used_bytes,
            elastic_memory_used_bytes: state.elastic_memory_used_bytes,
            scratch_used_bytes: state.scratch_used_bytes,
            forge_reader_permits_used: state.forge_reader_permits_used,
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
            omitted_generation_count: state.scribe_omitted_generation_count,
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
        let generation_total = state
            .scribe_generation_bytes
            .checked_add(state.scribe_omitted_generation_bytes)
            .ok_or_else(accounting_overflow)?;
        let role_total = state
            .scribe_memory_used_bytes
            .checked_add(state.oracle_memory_used_bytes)
            .and_then(|bytes| bytes.checked_add(state.forge_memory_used_bytes))
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
            .and_then(|bytes| {
                bytes.checked_add(
                    state
                        .forge_memory_used_bytes
                        .saturating_sub(plan.forge_floor_bytes),
                )
            })
            .ok_or_else(accounting_overflow)?;
        if category_total != state.scribe_memory_used_bytes
            || shard_total > state.scribe_memory_used_bytes
            || generation_total != state.scribe_memory_used_bytes
            || role_total > plan.managed_memory_bytes
            || expected_elastic != state.elastic_memory_used_bytes
        {
            // Emit the operands: which of the five identities broke is the
            // whole diagnosis, and it is unrecoverable from the error string.
            tracing::error!(
                category_total,
                shard_total,
                generation_total,
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
        metrics::gauge!("bifrost_resource_memory_bytes", "kind" => "forge_floor")
            .set(as_f64(snapshot.plan.forge_floor_bytes));
        metrics::gauge!("bifrost_resource_memory_bytes", "kind" => "forge_used")
            .set(as_f64(snapshot.forge_memory_used_bytes));
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
        let next_generation = state
            .scribe_generation_bytes
            .checked_add(usize::from(request.generation.is_some()) * request.bytes)
            .ok_or_else(accounting_overflow)?;
        let next_omitted = state
            .scribe_omitted_generation_bytes
            .checked_add(usize::from(request.generation.is_none()) * request.bytes)
            .ok_or_else(accounting_overflow)?;
        let next_omitted_count = state
            .scribe_omitted_generation_count
            .checked_add(usize::from(request.generation.is_none()))
            .ok_or_else(accounting_overflow)?;
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
        state.scribe_generation_bytes = next_generation;
        state.scribe_omitted_generation_bytes = next_omitted;
        state.scribe_omitted_generation_count = next_omitted_count;
        record_memory_transition("scribe", "acquired", next);
        Ok(ScribeMemoryLease {
            root: self.clone(),
            bytes: request.bytes,
            category: request.category,
            shard: request.shard,
            generation: request.generation,
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
        let exact_class_quantum = match request.query_class {
            QueryClass::Interactive => (
                ORACLE_PARTITION_MEMORY_BYTES,
                ORACLE_PARTITION_MEMORY_BYTES as u64,
                1,
            ),
            QueryClass::Analytical => (
                2 * ORACLE_PARTITION_MEMORY_BYTES,
                (2 * ORACLE_PARTITION_MEMORY_BYTES) as u64,
                2,
            ),
        };
        if (
            request.memory_bytes,
            request.scratch_bytes,
            request.slot_units,
        ) != exact_class_quantum
        {
            return Err(BifrostResourceError::InvalidPlan {
                detail: "Oracle query demand must match its class quantum".to_owned(),
            });
        }
        Ok(())
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
        let analytical_ceiling = role_ceiling.saturating_sub(ORACLE_PARTITION_MEMORY_BYTES);
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
        let target_partitions = oracle_target_partitions(
            plan.effective_cpu,
            request.local_ratio,
            request.memory_bytes,
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
        let memory_pool = bounded_memory_pool(request.memory_bytes);
        Ok(OracleQueryResources {
            query_class: request.query_class,
            memory_bytes: request.memory_bytes,
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

    /// Atomically acquires exact Forge elastic memory and scratch ownership.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Occupied`] without partial ownership when
    /// either request exceeds currently free capacity.
    pub(crate) fn try_acquire_forge(
        &self,
        memory_bytes: usize,
        scratch_bytes: u64,
        reader_permits: u16,
    ) -> Result<ForgeRewriteResources, BifrostResourceError> {
        let mut state = self.lock_state()?;
        let plan = self.plan();
        if plan.forge_floor_bytes == 0 {
            return Err(BifrostResourceError::InvalidPlan {
                detail: "Forge resources requested while the role is inactive".to_owned(),
            });
        }
        let next_forge = state
            .forge_memory_used_bytes
            .checked_add(memory_bytes)
            .ok_or_else(accounting_overflow)?;
        let prior_borrow = state
            .forge_memory_used_bytes
            .saturating_sub(plan.forge_floor_bytes);
        let next_borrow = next_forge.saturating_sub(plan.forge_floor_bytes);
        let added_elastic = next_borrow - prior_borrow;
        let next_memory = state
            .elastic_memory_used_bytes
            .checked_add(added_elastic)
            .ok_or_else(accounting_overflow)?;
        let next_scratch = state
            .scratch_used_bytes
            .checked_add(scratch_bytes)
            .ok_or_else(accounting_overflow)?;
        let next_readers = state
            .forge_reader_permits_used
            .checked_add(usize::from(reader_permits))
            .ok_or_else(accounting_overflow)?;
        if reader_permits == 0
            || next_memory > plan.elastic_memory_bytes
            || next_scratch > plan.scratch_limit_bytes
            || next_readers > plan.effective_cpu
        {
            record_memory_transition("forge", "refused", state.forge_memory_used_bytes);
            return Err(BifrostResourceError::Occupied {
                detail: "Forge request exceeds currently free memory, scratch, or reader permits"
                    .to_owned(),
            });
        }
        state.elastic_memory_used_bytes = next_memory;
        state.forge_memory_used_bytes = next_forge;
        state.scratch_used_bytes = next_scratch;
        state.forge_reader_permits_used = next_readers;
        record_memory_transition("forge", "acquired", next_forge);
        Ok(ForgeRewriteResources {
            memory_bytes,
            scratch_bytes,
            reader_permits: usize::from(reader_permits),
            memory_pool: bounded_memory_pool(memory_bytes),
            governor: self.clone(),
            release_result: None,
            volume_scratch: None,
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
    /// Optional durable-generation attribution retained across transformations.
    generation: Option<crate::scribe::seal_key::ScribeArtifactIdentity>,
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
        if self.generation.is_none() {
            let mut state = self.root.lock_state()?;
            state.scribe_omitted_generation_count = state
                .scribe_omitted_generation_count
                .checked_add(1)
                .ok_or_else(accounting_overflow)?;
        }
        self.bytes -= bytes;
        Ok(Self {
            root: self.root.clone(),
            bytes,
            category: self.category,
            shard: self.shard,
            generation: self.generation.clone(),
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
            || self.generation != other.generation
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
        if self.generation.is_none() {
            let mut state = self.root.lock_state()?;
            if state.scribe_omitted_generation_count == 0 {
                return Err(self.root.poison_locked(
                    &mut state,
                    "Scribe omitted-generation lease count underflow",
                ));
            }
            state.scribe_omitted_generation_count -= 1;
        }
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
        let generation = if self.generation.is_some() {
            state.scribe_generation_bytes
        } else {
            state.scribe_omitted_generation_bytes
        };
        let next_generation = generation
            .checked_add(growth)
            .ok_or_else(accounting_overflow)?;
        state.scribe_memory_used_bytes = next_total;
        state.elastic_memory_used_bytes = next_elastic;
        state.scribe_category_bytes[category] = next_category;
        if let (Some(shard), Some(total)) = (self.shard, next_shard) {
            state.scribe_shard_bytes.insert(shard, total);
        }
        if self.generation.is_some() {
            state.scribe_generation_bytes = next_generation;
        } else {
            state.scribe_omitted_generation_bytes = next_generation;
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
        let generation_total = if self.generation.is_some() {
            state.scribe_generation_bytes
        } else {
            state.scribe_omitted_generation_bytes
        };
        if state.scribe_memory_used_bytes < shrink
            || state.scribe_category_bytes[category] < shrink
            || shard_total.is_some_and(|total| total < shrink)
            || generation_total < shrink
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
        if self.generation.is_some() {
            state.scribe_generation_bytes -= shrink;
        } else {
            state.scribe_omitted_generation_bytes -= shrink;
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

    /// Clears shard and generation identity before joining an aggregate owner.
    ///
    /// # Errors
    ///
    /// Returns a poison error when the prior identity attribution cannot cover
    /// this lease or the omitted-generation counters overflow.
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
        if self.generation.take().is_some() {
            if state.scribe_generation_bytes < self.bytes {
                return Err(self
                    .root
                    .poison_locked(&mut state, "Scribe aggregate generation clear underflow"));
            }
            state.scribe_generation_bytes -= self.bytes;
            state.scribe_omitted_generation_bytes = state
                .scribe_omitted_generation_bytes
                .checked_add(self.bytes)
                .ok_or_else(accounting_overflow)?;
            state.scribe_omitted_generation_count = state
                .scribe_omitted_generation_count
                .checked_add(1)
                .ok_or_else(accounting_overflow)?;
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
        let attributed_generation = if self.generation.is_some() {
            state.scribe_generation_bytes
        } else {
            state.scribe_omitted_generation_bytes
        };
        if state.scribe_memory_used_bytes < self.bytes
            || state.scribe_category_bytes[category] < self.bytes
            || shard_bytes.is_some_and(|bytes| bytes < self.bytes)
            || attributed_generation < self.bytes
            || (self.generation.is_none() && state.scribe_omitted_generation_count == 0)
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
        if self.generation.is_some() {
            state.scribe_generation_bytes -= self.bytes;
        } else {
            state.scribe_omitted_generation_bytes -= self.bytes;
            state.scribe_omitted_generation_count -= 1;
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
    /// Exact bounded memory available to all query consumers.
    pub memory_bytes: usize,
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

/// Rewrite-lifetime exact elastic memory and scratch owner.
#[derive(Debug)]
pub struct ForgeRewriteResources {
    memory_bytes: usize,
    scratch_bytes: u64,
    /// Reader permits coupled to this exact attempt lease.
    reader_permits: usize,
    /// Operation-local pool nested inside this exact root lease.
    memory_pool: Arc<dyn MemoryPool>,
    governor: BifrostResourceGovernor,
    /// First terminal release result, retained for idempotent finalization.
    release_result: Option<ForgeResourceReleaseResult>,
    /// Exact physical Forge scratch ownership when live roots are registered.
    volume_scratch: Option<ScratchLease>,
}

impl ForgeRewriteResources {
    /// Returns the operation-local pool backed by this retained root lease.
    #[must_use]
    pub fn memory_pool(&self) -> Arc<dyn MemoryPool> {
        Arc::clone(&self.memory_pool)
    }

    /// Returns the exact scratch ceiling retained by this operation lease.
    #[must_use]
    pub fn scratch_bytes(&self) -> u64 {
        self.scratch_bytes
    }

    /// Splits one named memory child from this admitted rewrite pool.
    ///
    /// # Errors
    ///
    /// Returns `DataFusion` resource exhaustion when existing rewrite children
    /// leave insufficient capacity. This method never admits root capacity.
    pub(crate) fn try_split_memory(
        &self,
        consumer: &'static str,
        bytes: usize,
    ) -> Result<ForgeRewriteMemoryReservation, DataFusionError> {
        let reservation = MemoryConsumer::new(consumer).register(&self.memory_pool);
        reservation.try_grow(bytes)?;
        Ok(ForgeRewriteMemoryReservation {
            _reservation: reservation,
        })
    }

    /// Explicitly releases the complete lease exactly once.
    ///
    /// # Errors
    /// Returns a poisoned-accounting error when counters cannot be returned atomically.
    pub(crate) fn release(&mut self) -> Result<ForgeResourceReleaseResult, BifrostResourceError> {
        if let Some(result) = self.release_result {
            return Ok(result);
        }
        let mut state = match self.governor.lock_state() {
            Ok(state) => state,
            Err(error) => {
                self.release_result = Some(ForgeResourceReleaseResult::Poisoned);
                return Err(error);
            }
        };
        let plan = self.governor.plan();
        let released_elastic = released_elastic_bytes(
            state.forge_memory_used_bytes,
            plan.forge_floor_bytes,
            self.memory_bytes,
        );
        if state.forge_memory_used_bytes < self.memory_bytes
            || state.elastic_memory_used_bytes < released_elastic
            || state.scratch_used_bytes < self.scratch_bytes
            || state.forge_reader_permits_used < self.reader_permits
        {
            self.release_result = Some(ForgeResourceReleaseResult::Poisoned);
            return Err(self
                .governor
                .poison_locked(&mut state, "Forge release underflow"));
        }
        state.forge_memory_used_bytes -= self.memory_bytes;
        state.elastic_memory_used_bytes -= released_elastic;
        state.scratch_used_bytes -= self.scratch_bytes;
        state.forge_reader_permits_used -= self.reader_permits;
        state.memory_epoch = state.memory_epoch.wrapping_add(1);
        record_memory_transition("forge", "released", state.forge_memory_used_bytes);
        self.volume_scratch.take();
        self.release_result = Some(ForgeResourceReleaseResult::Released);
        drop(state);
        self.governor.inner.memory_changed.notify_waiters();
        Ok(ForgeResourceReleaseResult::Released)
    }

    /// Poisons the governor while deliberately retaining all leased counters.
    pub(crate) fn poison_without_release(&mut self) -> ForgeResourceReleaseResult {
        if let Some(result) = self.release_result {
            return result;
        }
        self.governor
            .poison("Forge runtime survived its attempt resource owner");
        self.release_result = Some(ForgeResourceReleaseResult::Poisoned);
        ForgeResourceReleaseResult::Poisoned
    }
}

/// Rewrite-local memory child split from one already admitted Forge attempt.
#[derive(Debug)]
pub(crate) struct ForgeRewriteMemoryReservation {
    /// Exact child registered in the attempt-local `DataFusion` pool.
    _reservation: MemoryReservation,
}

impl Drop for ForgeRewriteResources {
    /// Releases both Forge counters exactly once and poisons on underflow.
    fn drop(&mut self) {
        if self.release_result.is_some() {
            return;
        }
        if let Err(error) = self.release() {
            tracing::error!(%error, "Forge resource cleanup failed");
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

/// Builds a finite first-come, first-served pool for a nested resource envelope.
///
/// The caller must retain the outer [`BifrostResourceGovernor`] lease for at
/// least as long as this pool can be referenced. A zero limit is normalized to
/// one byte only for defensive construction; production admission never grants
/// a zero-byte envelope.
#[must_use]
pub(crate) fn bounded_memory_pool(limit_bytes: usize) -> Arc<dyn MemoryPool> {
    let pool: Arc<dyn MemoryPool> = Arc::new(TrackConsumersPool::new(
        GreedyMemoryPool::new(limit_bytes.max(1)),
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
impl MemoryPool for PeakTrackingMemoryPool {
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

    /// Returns a valid envelope for resource-ledger tests that do not execute it.
    fn envelope() -> vala_sql::row_types::forge_tasks::ForgeTaskEnvelope {
        vala_sql::row_types::forge_tasks::ForgeTaskEnvelope {
            version: vala_sql::row_types::forge_tasks::FORGE_ENVELOPE_VERSION,
            reader_permits: 1,
            decoded_batch_bytes: MIB as u64,
            decoded_input_bytes: MIB as u64,
            sort_working_bytes: 3 * MIB as u64,
            sort_merge_reservation_bytes: MIB as u64,
            encoder_buffer_bytes: 2 * MIB as u64,
            upload_chunk_bytes: MIB as u64,
            footer_encoded_bytes: 8 * MIB as u64,
            footer_decode_workspace_bytes: 32 * MIB as u64,
            sort_spill_bytes: MIB as u64,
            output_scratch_bytes: MIB as u64,
        }
    }

    fn policy(roles: &[BifrostRole]) -> BifrostResourcePolicy {
        BifrostResourcePolicy {
            roles: roles.iter().copied().collect(),
            memory_limit_bytes: None,
            unmanaged_reserve_bytes: None,
            scratch_limit_bytes: None,
            effective_cpu: None,
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

    /// Builds one exact interactive query quantum for root-ledger tests.
    fn interactive_query(local_ratio: f64) -> OracleResourceRequest {
        OracleResourceRequest {
            query_class: QueryClass::Interactive,
            memory_bytes: ORACLE_PARTITION_MEMORY_BYTES,
            scratch_bytes: ORACLE_PARTITION_MEMORY_BYTES as u64,
            slot_units: 1,
            local_ratio,
        }
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
        assert!(roles.forge().is_none());
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
        assert_eq!(worker.memory_bytes(), ORACLE_PARTITION_MEMORY_BYTES);
        assert_eq!(
            oracle
                .snapshot()
                .expect("worker snapshot")
                .elastic_memory_used_bytes,
            0
        );
        assert!(oracle.metadata().try_acquire_footer_slot().is_err());
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
            ORACLE_PARTITION_MEMORY_BYTES
        );
        finish.wait();
        for join in joins {
            join.join().expect("Oracle race thread");
        }
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
        let scribe = temp.path().join("scribe-output-scratch");
        let forge = temp.path().join("forge");
        let oracle = temp.path().join("oracle");
        for path in [&wal, &scribe, &forge, &oracle] {
            fs::create_dir(path).expect("registered volume root");
        }
        fs::write(wal.join("retained.wal"), [0_u8; 16]).expect("retained WAL fixture");
        let health = BifrostResourceHealth::default();
        let governor = BifrostVolumeGovernor::register(
            BifrostVolumeRoots {
                wal,
                scribe_output_scratch: scribe,
                forge_scratch: forge,
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
            assert!(capabilities.forge.try_acquire(33).is_err());
            drop(provisional);
        }
        let scratch = capabilities
            .forge
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
        let scribe = temp.path().join("scribe-output-scratch");
        let forge = temp.path().join("forge");
        let oracle = temp.path().join("oracle");
        for path in [&wal, &scribe, &forge, &oracle] {
            fs::create_dir(path).expect("registered volume root");
        }
        let governor = BifrostVolumeGovernor::register(
            BifrostVolumeRoots {
                wal,
                scribe_output_scratch: scribe,
                forge_scratch: forge,
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
                        .forge
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

    /// Scribe scratch cancellation and restart cleanup stay inside the owned namespace.
    ///
    /// # Panics
    ///
    /// Panics when deterministic namespace setup or cleanup fails.
    #[test]
    fn bifrost_volume_governor_scribe_namespace_is_exactly_scoped() {
        let temp = tempfile::tempdir().expect("temporary volume root");
        let wal = temp.path().join("wal");
        let scribe = wal.join("scribe-output-scratch");
        let forge = wal.join("forge");
        let oracle = wal.join("oracle");
        for path in [&wal, &scribe, &forge, &oracle] {
            fs::create_dir_all(path).expect("registered volume root");
        }
        let retained_wal = wal.join("retained.wal");
        let peer = scribe.join("peer-owned");
        let stale = scribe.join("scribe-runtime-old-7-0");
        fs::write(&retained_wal, [1_u8]).expect("retained WAL");
        fs::create_dir(&peer).expect("peer directory");
        fs::create_dir(&stale).expect("stale runtime directory");

        let governor = BifrostVolumeGovernor::register(
            BifrostVolumeRoots {
                wal,
                scribe_output_scratch: scribe.clone(),
                forge_scratch: forge,
                oracle_scratch: oracle,
            },
            1024,
            BifrostResourceHealth::default(),
        )
        .expect("registered roots reconcile process-owned scratch");
        assert!(!stale.exists());
        assert!(peer.exists());
        assert!(retained_wal.exists());

        let scratch = governor
            .capabilities()
            .scribe_output
            .create_scribe_generation("stream_1", 9, 32)
            .expect("generation scratch");
        let owned = scratch.path().to_owned();
        assert_eq!(owned.parent(), Some(scribe.as_path()));
        assert!(
            owned
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("scribe-runtime-stream_1-9-"))
        );
        drop(scratch);
        assert!(!owned.exists());
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
        let scribe = wal.join("scribe-output-scratch");
        let forge = wal.join("forge");
        let oracle = wal.join("oracle");
        for path in [&wal, &scribe, &forge, &oracle] {
            fs::create_dir_all(path).expect("registered volume root");
        }
        let health = BifrostResourceHealth::default();
        let governor = BifrostVolumeGovernor::register(
            BifrostVolumeRoots {
                wal,
                scribe_output_scratch: scribe,
                forge_scratch: forge,
                oracle_scratch: oracle,
            },
            1024,
            health.clone(),
        )
        .expect("registered roots");
        let mut scratch = governor
            .capabilities()
            .scribe_output
            .create_scribe_generation("stream", 4, 32)
            .expect("generation scratch");
        let attempts = std::cell::Cell::new(0);
        let error = scratch
            .cleanup_owned_prefix_with(|| {
                attempts.set(attempts.get() + 1);
                Err(std::io::Error::other("injected unlink failure"))
            })
            .expect_err("exhausted cleanup must fail closed");
        assert!(matches!(error, BifrostResourceError::Poisoned { .. }));
        assert_eq!(attempts.get(), 3);
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
            let owner = oracle
                .try_acquire_worker(OracleWorkerClass::Interactive)
                .expect("Oracle grant");
            assert!(
                oracle
                    .try_acquire_worker(OracleWorkerClass::Interactive)
                    .is_err()
            );
            drop(owner);

            let temp = tempfile::tempdir().expect("temporary volume root");
            let wal = temp.path().join("wal");
            let scribe = temp.path().join("scribe-output-scratch");
            let forge = temp.path().join("forge");
            let oracle_root = temp.path().join("oracle");
            for path in [&wal, &scribe, &forge, &oracle_root] {
                fs::create_dir(path).expect("registered volume root");
            }
            let volumes = BifrostVolumeGovernor::register(
                BifrostVolumeRoots {
                    wal,
                    scribe_output_scratch: scribe,
                    forge_scratch: forge,
                    oracle_scratch: oracle_root,
                },
                64,
                BifrostResourceHealth::default(),
            )
            .expect("volume plan");
            let capability = volumes.capabilities().forge;
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
            "volume_class=\"forge\"",
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
        let scribe = disk.path().join("scribe-output-scratch");
        let forge = disk.path().join("forge");
        let oracle = memory.path().join("oracle");
        for path in [&wal, &scribe, &forge, &oracle] {
            fs::create_dir(path).expect("registered volume root");
        }
        if fs::metadata(&forge).expect("Forge metadata").dev()
            == fs::metadata(&oracle).expect("Oracle metadata").dev()
            || filesystem_available_bytes(&oracle)
                .map_or(true, |available| available < MIN_SCRATCH_FREE_BYTES + 64)
        {
            return;
        }
        let governor = BifrostVolumeGovernor::register(
            BifrostVolumeRoots {
                wal,
                scribe_output_scratch: scribe,
                forge_scratch: forge,
                oracle_scratch: oracle,
            },
            64,
            BifrostResourceHealth::default(),
        )
        .expect("distinct roots register");
        let capabilities = governor.capabilities();
        let forge_lease = capabilities
            .forge
            .try_acquire(64)
            .expect("Forge consumes its device boundary");
        let oracle_lease = capabilities
            .oracle
            .try_acquire(64)
            .expect("Oracle independently consumes its device boundary");
        assert!(capabilities.forge.try_acquire(1).is_err());
        assert!(capabilities.oracle.try_acquire(1).is_err());
        drop((forge_lease, oracle_lease));
    }

    /// Enabled roles alone receive protected floors and elastic arithmetic is exact.
    #[test]
    fn resource_plan_reserves_only_enabled_role_floors() {
        let gib = 1024 * MIB;
        let cases = [
            (&[BifrostRole::Oracle][..], 0, 256 * MIB, 0, 512 * MIB),
            (&[BifrostRole::Scribe][..], 256 * MIB, 0, 0, 512 * MIB),
            (&[BifrostRole::Forge][..], 0, 0, 64 * MIB, 704 * MIB),
            (
                &[BifrostRole::Scribe, BifrostRole::Oracle][..],
                256 * MIB,
                256 * MIB,
                0,
                256 * MIB,
            ),
        ];
        for (roles, scribe, oracle, forge, elastic) in cases {
            let runtime = BifrostRuntimeResources::from_snapshot(snapshot(gib), policy(roles))
                .expect("resource plan must fit");
            let plan = runtime.plan();
            assert_eq!(plan.managed_memory_bytes, 768 * MIB);
            assert_eq!(plan.scribe_floor_bytes, scribe);
            assert_eq!(plan.oracle_floor_bytes, oracle);
            assert_eq!(plan.forge_floor_bytes, forge);
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
        assert!(oracle.try_acquire_query(interactive_query(1.0)).is_err());
        let occupied = oracle.snapshot().expect("snapshot");
        assert!(occupied.oracle_query_active);
        drop(first);
        assert_eq!(
            oracle.snapshot().expect("released snapshot"),
            ResourceSnapshot {
                plan: roles.plan(),
                scribe_memory_used_bytes: 0,
                oracle_memory_used_bytes: 0,
                forge_memory_used_bytes: 0,
                elastic_memory_used_bytes: 0,
                scratch_used_bytes: 0,
                forge_reader_permits_used: 0,
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
        assert_eq!(query.memory_bytes, ORACLE_PARTITION_MEMORY_BYTES);
        let pool = query.memory_pool();
        let reservation = MemoryConsumer::new("oracle-test").register(&pool);
        reservation
            .try_grow(query.memory_bytes)
            .expect("aggregate grant is usable");
        assert!(reservation.try_grow(1).is_err());
        reservation.shrink(query.memory_bytes);
        assert_eq!(pool.reserved(), 0);
        drop(query);
        assert!(!roles.snapshot().expect("snapshot").oracle_query_active);
    }

    /// Forge's runtime pool remains nested in and bounded by its retained lease.
    #[test]
    fn forge_harness_pool_is_issued_by_operation_lease() {
        let roles = BifrostRuntimeResources::composed_for_test(
            1024 * MIB,
            512 * MIB as u64,
            [BifrostRole::Forge],
        );
        let forge = roles.forge().expect("Forge capability");
        let lease = forge
            .try_acquire_rewrite(ForgeRewriteRequest {
                envelope: envelope(),
                memory_bytes: 128 * MIB,
                scratch_bytes: 64 * MIB as u64,
                reader_permits: 1,
            })
            .expect("Forge operation lease");
        let pool = lease.memory_pool();
        let reservation = MemoryConsumer::new("forge-operation-test").register(&pool);
        reservation
            .try_grow(128 * MIB)
            .expect("lease pool accepts exact capacity");
        assert!(reservation.try_grow(1).is_err());
        reservation.shrink(128 * MIB);
        drop(lease);
        let released = forge.snapshot().expect("released Forge snapshot");
        assert_eq!(released.elastic_memory_used_bytes, 0);
        assert_eq!(released.scratch_used_bytes, 0);
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
                        replayed_plan.forge_floor_bytes,
                        replayed_plan.elastic_memory_bytes,
                    ),
                    (
                        live_plan.memory_limit_bytes,
                        live_plan.effective_cpu,
                        live_plan.unmanaged_reserve_bytes,
                        live_plan.managed_memory_bytes,
                        live_plan.scribe_floor_bytes,
                        live_plan.oracle_floor_bytes,
                        live_plan.forge_floor_bytes,
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
        let forge = roles.forge().expect("Forge capability");
        assert!(oracle.shares_root_with(&roles));
        assert!(forge.shares_root_with(&roles));
        assert!(runtime.shares_root_with(&roles));

        let lease = forge
            .try_acquire_rewrite(ForgeRewriteRequest {
                envelope: envelope(),
                memory_bytes: 64 * MIB,
                scratch_bytes: 64 * MIB as u64,
                reader_permits: 1,
            })
            .expect("Forge lease");
        let occupied = oracle.snapshot().expect("Oracle observes the shared root");
        assert_eq!(occupied.forge_memory_used_bytes, 64 * MIB);
        assert_eq!(occupied.elastic_memory_used_bytes, 0);
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
                generation: None,
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
        assert_eq!(attribution.omitted_generation_count, 1);
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
                generation: None,
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
                generation: None,
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
        let analytical = oracle
            .try_acquire_worker(OracleWorkerClass::Analytical)
            .expect("two-quanta worker");
        assert_eq!(analytical.memory_bytes(), 2 * ORACLE_PARTITION_MEMORY_BYTES);
        assert!(
            oracle
                .try_acquire_worker(OracleWorkerClass::Interactive)
                .is_err()
        );
        drop(analytical);
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
        assert_eq!(occupied.oracle_query_memory_used_bytes, 512 * MIB);
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

    /// Analytical admission preserves one complete interactive query quantum.
    #[test]
    fn analytical_capacity_preserves_one_interactive_quantum() {
        let roles = BifrostRuntimeResources::composed_for_test(
            1280 * MIB,
            1024 * MIB as u64,
            [BifrostRole::Oracle],
        );
        let oracle = roles.oracle().expect("Oracle capability");
        let analytical = oracle
            .try_acquire_query(OracleResourceRequest {
                query_class: QueryClass::Analytical,
                memory_bytes: 2 * ORACLE_PARTITION_MEMORY_BYTES,
                scratch_bytes: (2 * ORACLE_PARTITION_MEMORY_BYTES) as u64,
                slot_units: 2,
                local_ratio: 0.0,
            })
            .expect("analytical query below protected reserve");
        let occupied = oracle.snapshot().expect("one analytical owner");
        let ordinary_memory_remaining = occupied
            .plan
            .oracle_floor_bytes
            .checked_add(occupied.plan.elastic_memory_bytes)
            .and_then(|total| total.checked_sub(occupied.oracle_memory_used_bytes))
            .expect("ordinary Oracle memory remainder");
        let scratch_remaining = occupied
            .plan
            .scratch_limit_bytes
            .checked_sub(occupied.scratch_used_bytes)
            .expect("Oracle scratch remainder");
        let slots_remaining = oracle_worker_slots(occupied.plan)
            .expect("Oracle slot ceiling")
            .checked_sub(occupied.oracle_query_slot_units as usize)
            .expect("Oracle slot remainder");
        assert_eq!(ordinary_memory_remaining, 2 * ORACLE_PARTITION_MEMORY_BYTES);
        assert_eq!(
            scratch_remaining,
            (2 * ORACLE_PARTITION_MEMORY_BYTES) as u64
        );
        assert_eq!(slots_remaining, 2);

        let refused = oracle.try_acquire_query(OracleResourceRequest {
            query_class: QueryClass::Analytical,
            memory_bytes: 2 * ORACLE_PARTITION_MEMORY_BYTES,
            scratch_bytes: (2 * ORACLE_PARTITION_MEMORY_BYTES) as u64,
            slot_units: 2,
            local_ratio: 0.0,
        });
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
        assert_eq!(snapshot.oracle_analytical_queries, 1);
        assert_eq!(snapshot.oracle_query_slot_units, 3);
        drop((analytical, interactive));
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
        let forge = roles.forge().expect("Forge capability");
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
                    generation: None,
                })
                .expect("Scribe lease one"),
            scribe
                .try_acquire_memory(ScribeMemoryRequest {
                    bytes: 64 * MIB,
                    category: ScribeMemoryCategory::Queued,
                    shard: Some(1),
                    generation: None,
                })
                .expect("Scribe lease two"),
        ];
        let forge_leases = [
            forge
                .try_acquire_rewrite(ForgeRewriteRequest {
                    envelope: envelope(),
                    memory_bytes: 32 * MIB,
                    scratch_bytes: 32 * MIB as u64,
                    reader_permits: 1,
                })
                .expect("Forge lease one"),
            forge
                .try_acquire_rewrite(ForgeRewriteRequest {
                    envelope: envelope(),
                    memory_bytes: 32 * MIB,
                    scratch_bytes: 32 * MIB as u64,
                    reader_permits: 1,
                })
                .expect("Forge lease two"),
        ];
        let snapshot = roles.snapshot().expect("shared-root snapshot");
        assert!(
            snapshot.scribe_memory_used_bytes
                + snapshot.oracle_memory_used_bytes
                + snapshot.forge_memory_used_bytes
                <= snapshot.plan.managed_memory_bytes
        );
        assert_eq!(
            snapshot.elastic_memory_used_bytes,
            ORACLE_PARTITION_MEMORY_BYTES
        );
        drop((queries, scribe_leases, forge_leases));
        let released = roles.snapshot().expect("shared-root release");
        assert_eq!(released.scribe_memory_used_bytes, 0);
        assert_eq!(released.oracle_memory_used_bytes, 0);
        assert_eq!(released.forge_memory_used_bytes, 0);
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
        let occupied = oracle.snapshot().expect("occupied snapshot");
        assert!(oracle.try_acquire_query(interactive_query(0.0)).is_err());
        assert_eq!(
            oracle.snapshot().expect("post-refusal snapshot"),
            occupied,
            "a refusal must not mutate any counter"
        );
        drop(held);
        let released = oracle.snapshot().expect("released snapshot");
        assert_eq!(released.elastic_memory_used_bytes, 0);
        assert_eq!(released.scratch_used_bytes, 0);
        assert!(!released.oracle_query_active);
        assert!(
            oracle.try_acquire_query(interactive_query(0.0)).is_ok(),
            "the root remains usable after a refusal"
        );
    }

    /// A refused Forge rewrite leaves both counters exactly as it found them.
    #[test]
    fn forge_capability_refusal_is_atomic() {
        let roles = BifrostRuntimeResources::composed_for_test(
            1024 * MIB,
            512 * MIB as u64,
            [BifrostRole::Forge],
        );
        let forge = roles.forge().expect("Forge capability");
        let plan = roles.plan();
        let baseline = forge.snapshot().expect("baseline");

        assert!(
            forge
                .try_acquire_rewrite(ForgeRewriteRequest {
                    envelope: envelope(),
                    memory_bytes: plan.forge_floor_bytes + plan.elastic_memory_bytes + 1,
                    scratch_bytes: 1,
                    reader_permits: 1,
                })
                .is_err(),
            "memory beyond the elastic pool must be refused"
        );
        assert_eq!(forge.snapshot().expect("after memory refusal"), baseline);

        assert!(
            forge
                .try_acquire_rewrite(ForgeRewriteRequest {
                    envelope: envelope(),
                    memory_bytes: 1,
                    scratch_bytes: plan.scratch_limit_bytes + 1,
                    reader_permits: 1,
                })
                .is_err(),
            "scratch beyond the disposable ceiling must be refused"
        );
        assert_eq!(forge.snapshot().expect("after scratch refusal"), baseline);

        assert!(
            forge
                .try_acquire_rewrite(ForgeRewriteRequest {
                    envelope: envelope(),
                    memory_bytes: 1,
                    scratch_bytes: 1,
                    reader_permits: u16::try_from(plan.effective_cpu + 1).unwrap_or(u16::MAX),
                })
                .is_err(),
            "reader permits beyond live CPU capacity must be refused atomically"
        );
        assert_eq!(forge.snapshot().expect("after reader refusal"), baseline);
    }

    /// Concurrent Forge grants contend on reader permits in the same atomic ledger.
    #[test]
    fn forge_reader_permits_are_atomic_under_contention() {
        let roles = BifrostRuntimeResources::composed_for_test(
            1024 * MIB,
            512 * MIB as u64,
            [BifrostRole::Forge],
        );
        let forge = roles.forge().expect("Forge capability");
        let permits = u16::try_from(roles.plan().effective_cpu).unwrap_or(u16::MAX);
        let lease = forge
            .try_acquire_rewrite(ForgeRewriteRequest {
                envelope: envelope(),
                memory_bytes: 1,
                scratch_bytes: 1,
                reader_permits: permits,
            })
            .expect("first lease owns every reader permit");
        let held = forge.snapshot().expect("held snapshot");
        assert_eq!(held.forge_reader_permits_used, usize::from(permits));
        assert!(
            forge
                .try_acquire_rewrite(ForgeRewriteRequest {
                    envelope: envelope(),
                    memory_bytes: 1,
                    scratch_bytes: 1,
                    reader_permits: 1,
                })
                .is_err()
        );
        assert_eq!(forge.snapshot().expect("refusal snapshot"), held);
        drop(lease);
        assert_eq!(
            forge
                .snapshot()
                .expect("released")
                .forge_reader_permits_used,
            0
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
            .try_grow(query.memory_bytes)
            .expect("the lease pool admits exactly its grant");
        assert!(reservation.try_grow(1).is_err());
        reservation.shrink(query.memory_bytes);
        drop(query);
        assert_eq!(
            oracle.snapshot().expect("released"),
            baseline,
            "dropping the query lease must restore every baseline"
        );
    }

    /// The Forge runtime pool is the lease's own, and drop restores baselines.
    #[test]
    fn forge_lease_pool_identity_and_drop_cleanup() {
        let roles = BifrostRuntimeResources::composed_for_test(
            1024 * MIB,
            512 * MIB as u64,
            [BifrostRole::Forge],
        );
        let forge = roles.forge().expect("Forge capability");
        let baseline = forge.snapshot().expect("baseline");
        let lease = forge
            .try_acquire_rewrite(ForgeRewriteRequest {
                envelope: envelope(),
                memory_bytes: 128 * MIB,
                scratch_bytes: 64 * MIB as u64,
                reader_permits: 1,
            })
            .expect("Forge operation lease");
        let pool = lease.memory_pool();
        assert!(
            Arc::ptr_eq(&pool, &lease.memory_pool()),
            "the lease must issue one stable pool"
        );
        let reservation = MemoryConsumer::new("forge-lease-identity").register(&pool);
        reservation
            .try_grow(128 * MIB)
            .expect("the lease pool admits exactly its grant");
        assert!(reservation.try_grow(1).is_err());
        reservation.shrink(128 * MIB);
        assert_eq!(lease.scratch_bytes(), 64 * MIB as u64);
        drop(lease);
        assert_eq!(
            forge.snapshot().expect("released"),
            baseline,
            "dropping the operation lease must restore every baseline"
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
    fn scribe_floor_survives_oracle_and_forge_elastic_pressure() {
        let roles = BifrostRuntimeResources::composed_for_test(
            1024 * MIB,
            512 * MIB as u64,
            [BifrostRole::Scribe, BifrostRole::Oracle, BifrostRole::Forge],
        );
        let plan = roles.plan();
        assert_eq!(plan.scribe_floor_bytes, ROLE_MEMORY_FLOOR_BYTES);
        let scribe = roles.scribe().expect("Scribe capability");
        let oracle = roles.oracle().expect("Oracle capability");
        let forge = roles.forge().expect("Forge capability");
        let scribe_owner = scribe
            .try_acquire_memory(ScribeMemoryRequest {
                bytes: 300 * MIB,
                category: crate::scribe::memory::MemoryCategory::Active,
                shard: None,
                generation: None,
            })
            .expect("Scribe uses its floor and borrows elastic memory");
        let with_scribe = scribe.snapshot().expect("Scribe ownership snapshot");
        assert_eq!(with_scribe.scribe_memory_used_bytes, 300 * MIB);
        assert_eq!(with_scribe.elastic_memory_used_bytes, 44 * MIB);
        let query = oracle
            .try_acquire_query(interactive_query(0.0))
            .expect("Oracle owns only its floor and shared elastic memory");
        assert_eq!(query.memory_bytes, ORACLE_PARTITION_MEMORY_BYTES);
        let concurrent_forge = forge
            .try_acquire_rewrite(ForgeRewriteRequest {
                envelope: envelope(),
                memory_bytes: 1,
                scratch_bytes: 1,
                reader_permits: 1,
            })
            .expect("Forge retains its floor while Oracle owns an exact query");
        assert_eq!(roles.plan().scribe_floor_bytes, ROLE_MEMORY_FLOOR_BYTES);
        drop(concurrent_forge);
        drop(query);
        drop(scribe_owner);
        let forge_owner = forge
            .try_acquire_rewrite(ForgeRewriteRequest {
                envelope: envelope(),
                memory_bytes: plan.forge_floor_bytes + plan.elastic_memory_bytes,
                scratch_bytes: plan.scratch_limit_bytes,
                reader_permits: 1,
            })
            .expect("Forge may own all elastic resources after Oracle releases");
        assert_eq!(roles.plan().scribe_floor_bytes, ROLE_MEMORY_FLOOR_BYTES);
        drop(forge_owner);
        let snapshot = roles.snapshot().expect("released resource snapshot");
        assert_eq!(snapshot.elastic_memory_used_bytes, 0);
        assert_eq!(snapshot.scratch_used_bytes, 0);
    }
}
