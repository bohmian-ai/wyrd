//! Portable pod resource detection and cross-role Bifrost admission.
//!
//! [`BifrostResourceGovernor`] is the single process-local authority for the
//! memory and disposable scratch resources shared by Scribe, Oracle, and
//! Forge. Detection uses only process-visible operating-system interfaces;
//! deployment systems may reduce detected limits through absolute overrides
//! but are never part of the allocation policy.

use std::collections::BTreeSet;
use std::fs;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
#[cfg(any(test, feature = "test-support"))]
use std::sync::LazyLock;
#[cfg(any(test, feature = "test-support"))]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use datafusion::error::DataFusionError;
use datafusion::execution::memory_pool::{
    GreedyMemoryPool, MemoryConsumer, MemoryLimit, MemoryPool, MemoryReservation,
    TrackConsumersPool,
};
use num_traits::ToPrimitive;
use rustix::fs::statvfs;

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
    /// Memory available for elastic Oracle or Forge ownership.
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

/// Point-in-time live resource ownership.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceSnapshot {
    /// Immutable boot calculation.
    pub plan: ResourcePlan,
    /// Total live Scribe ownership, including its protected floor.
    pub scribe_memory_used_bytes: usize,
    /// Elastic memory held by Oracle or Forge.
    pub elastic_memory_used_bytes: usize,
    /// Disposable scratch held by Oracle or Forge.
    pub scratch_used_bytes: u64,
    /// Concurrent Forge input-reader permits held by active rewrites.
    pub forge_reader_permits_used: usize,
    /// Whether the sole Oracle query owner is active.
    pub oracle_query_active: bool,
}

/// One complete Oracle query request.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OracleResourceRequest {
    /// Fraction of pinned input bytes expected to be local, in `[0, 1]`.
    pub local_ratio: f64,
}

#[derive(Debug, Default)]
struct ResourceState {
    scribe_memory_used_bytes: usize,
    elastic_memory_used_bytes: usize,
    scratch_used_bytes: u64,
    forge_reader_permits_used: usize,
    oracle_query_active: bool,
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
    /// Scribe compatibility ledger backed by the same root authority.
    scribe_memory: crate::scribe::memory::BifrostMemoryGovernor,
}

impl BifrostRuntimeResources {
    /// Detects process-visible resources and constructs the shared role graph.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError`] when detection, policy validation, or
    /// construction of the root-backed Scribe compatibility ledger fails.
    pub fn detect(policy: BifrostResourcePolicy) -> Result<Self, BifrostResourceError> {
        let snapshot = detect_snapshot(&policy.scratch_root, policy.memory_limit_bytes)?;
        Self::from_snapshot(snapshot, policy)
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
    /// policy or the root-backed Scribe compatibility ledger cannot be built.
    pub fn from_snapshot(
        snapshot: SystemResourceSnapshot,
        policy: BifrostResourcePolicy,
    ) -> Result<Self, BifrostResourceError> {
        let governor = BifrostResourceGovernor::from_snapshot(snapshot, policy)?;
        let scribe_memory =
            crate::scribe::memory::BifrostMemoryGovernor::from_resource_governor(governor.clone())
                .map_err(|error| BifrostResourceError::InvalidPlan {
                    detail: format!("Scribe compatibility ledger construction failed: {error}"),
                })?;
        Ok(Self {
            governor,
            scribe_memory,
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
        Ok(BifrostRoleResources {
            memory: self.scribe_memory.clone(),
            scribe: (plan.scribe_floor_bytes > 0).then(|| ScribeResources {
                memory: self.scribe_memory.clone(),
                governor: self.governor.clone(),
            }),
            oracle: (plan.oracle_floor_bytes > 0).then(|| OracleResources {
                governor: self.governor.clone(),
            }),
            forge: self
                .governor
                .is_enabled(BifrostRole::Forge)
                .then(|| ForgeResources {
                    governor: self.governor.clone(),
                }),
            governor: self.governor.clone(),
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

/// Enabled narrow role capabilities issued from one composition operation.
///
/// The result retains only the capabilities the checked policy activated. It
/// deliberately exposes no raw-root accessor and no generic pool factory, so a
/// production-equivalent caller cannot construct or clone a sibling root.
#[derive(Debug, Clone)]
pub struct BifrostRoleResources {
    /// Root-backed shared memory ledger every role's `DataFusion` ceiling derives
    /// from, retained here so an Oracle-only or Forge-only node still projects
    /// the same parent bound without enabling the Scribe role.
    memory: crate::scribe::memory::BifrostMemoryGovernor,
    scribe: Option<ScribeResources>,
    oracle: Option<OracleResources>,
    forge: Option<ForgeResources>,
    governor: BifrostResourceGovernor,
}

impl BifrostRoleResources {
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

    /// Returns the shared memory ledger backing every role's `DataFusion` ceiling.
    ///
    /// This is the same root-backed ledger the Scribe capability exposes; it is
    /// available from the composition itself because the query memory pool
    /// bound is a process-level property, not a Scribe-role property.
    #[must_use]
    pub fn memory_ledger(&self) -> crate::scribe::memory::BifrostMemoryGovernor {
        self.memory.clone()
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
    memory: crate::scribe::memory::BifrostMemoryGovernor,
    governor: BifrostResourceGovernor,
}

impl ScribeResources {
    /// Returns the root-backed Scribe ingest/persistence memory ledger.
    #[must_use]
    pub fn memory_governor(&self) -> crate::scribe::memory::BifrostMemoryGovernor {
        self.memory.clone()
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
}

impl OracleResources {
    /// Atomically acquires the complete currently-free Oracle query envelope.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Occupied`] when another query owns the
    /// envelope or the exact memory/scratch minimum cannot be owned, and
    /// [`BifrostResourceError::InvalidPlan`] for invalid partition inputs. No
    /// counter changes on refusal.
    pub fn try_acquire_query(
        &self,
        request: OracleResourceRequest,
    ) -> Result<OracleQueryResources, BifrostResourceError> {
        self.governor.try_acquire_oracle(request)
    }

    /// Returns the crate-private root ledger backing this capability.
    ///
    /// Nested query-local consumers must poison the same root they lease from.
    /// This accessor is crate-private: it never widens the external surface and
    /// cannot be used to construct a sibling root.
    #[must_use]
    pub(crate) fn governor(&self) -> BifrostResourceGovernor {
        self.governor.clone()
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
}

impl ForgeResources {
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
        self.governor.try_acquire_forge(
            request.memory_bytes,
            request.scratch_bytes,
            request.reader_permits,
        )
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
        let scribe_floor_bytes = usize::from(policy.roles.contains(&BifrostRole::Scribe))
            .checked_mul(ROLE_MEMORY_FLOOR_BYTES)
            .ok_or_else(accounting_overflow)?;
        let oracle_floor_bytes = usize::from(policy.roles.contains(&BifrostRole::Oracle))
            .checked_mul(ROLE_MEMORY_FLOOR_BYTES)
            .ok_or_else(accounting_overflow)?;
        let protected = scribe_floor_bytes
            .checked_add(oracle_floor_bytes)
            .ok_or_else(accounting_overflow)?;
        let elastic_memory_bytes = managed_memory_bytes.checked_sub(protected).ok_or_else(|| {
            BifrostResourceError::InvalidPlan {
                detail: format!("managed memory {managed_memory_bytes} cannot cover enabled role floors {protected}"),
            }
        })?;
        if policy.roles.contains(&BifrostRole::Forge)
            && elastic_memory_bytes < FORGE_MEMORY_FLOOR_BYTES
        {
            return Err(BifrostResourceError::InvalidPlan {
                detail: format!(
                    "Forge requires at least {FORGE_MEMORY_FLOOR_BYTES} elastic memory bytes; only {elastic_memory_bytes} are available"
                ),
            });
        }
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
        Ok(Self {
            inner: Arc::new(ResourceGovernorInner {
                plan: ResourcePlan {
                    memory_limit_bytes,
                    effective_cpu,
                    unmanaged_reserve_bytes,
                    managed_memory_bytes,
                    scribe_floor_bytes,
                    oracle_floor_bytes,
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
            }),
        })
    }

    /// Reports whether the checked policy stage activated `role`.
    #[must_use]
    pub(crate) fn is_enabled(&self, role: BifrostRole) -> bool {
        self.inner.roles.contains(&role)
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

    /// Captures exact live elastic and scratch ownership.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Poisoned`] when the shared lock or prior
    /// accounting failure makes a trustworthy snapshot unavailable.
    pub(crate) fn snapshot(&self) -> Result<ResourceSnapshot, BifrostResourceError> {
        let state = self.lock_state()?;
        Ok(ResourceSnapshot {
            plan: self.plan(),
            scribe_memory_used_bytes: state.scribe_memory_used_bytes,
            elastic_memory_used_bytes: state.elastic_memory_used_bytes,
            scratch_used_bytes: state.scratch_used_bytes,
            forge_reader_permits_used: state.forge_reader_permits_used,
            oracle_query_active: state.oracle_query_active,
        })
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

    /// Reserves Scribe memory while protecting every other active-role floor.
    ///
    /// Bytes within the Scribe floor do not consume elastic capacity. Growth
    /// above the floor atomically consumes only the incremental elastic bytes.
    ///
    /// # Errors
    ///
    /// Returns a typed refusal without mutation when Scribe is inactive,
    /// arithmetic fails, or shared elastic memory cannot cover the request.
    pub(crate) fn try_acquire_scribe(
        &self,
        bytes: usize,
    ) -> Result<ScribeResourceGrowth, BifrostResourceError> {
        let mut state = self.lock_state()?;
        let plan = self.plan();
        if plan.scribe_floor_bytes == 0 {
            return Err(BifrostResourceError::InvalidPlan {
                detail: "Scribe resources requested while the role is inactive".to_owned(),
            });
        }
        let next = state
            .scribe_memory_used_bytes
            .checked_add(bytes)
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
        if next_elastic > plan.elastic_memory_bytes {
            return Err(BifrostResourceError::Occupied {
                detail: "Scribe request exceeds protected floor plus free elastic memory"
                    .to_owned(),
            });
        }
        state.scribe_memory_used_bytes = next;
        state.elastic_memory_used_bytes = next_elastic;
        Ok(ScribeResourceGrowth {
            bytes,
            governor: self.clone(),
            committed: false,
        })
    }

    /// Validates a Scribe release without mutating shared counters.
    ///
    /// # Errors
    ///
    /// Returns a poison error when live Scribe ownership cannot cover `bytes`.
    pub(crate) fn preflight_release_scribe(
        &self,
        bytes: usize,
    ) -> Result<(), BifrostResourceError> {
        let mut state = self.lock_state()?;
        if state.scribe_memory_used_bytes < bytes {
            return Err(Self::poison_locked(
                &mut state,
                "Scribe resource release underflow",
            ));
        }
        Ok(())
    }

    /// Releases exact Scribe ownership after all coupled ledgers preflight.
    ///
    /// # Errors
    ///
    /// Returns a poison error on underflow; callers fail closed because a
    /// coupled release may already have made partial progress.
    pub(crate) fn release_scribe(&self, bytes: usize) -> Result<(), BifrostResourceError> {
        let mut state = self.lock_state()?;
        if state.scribe_memory_used_bytes < bytes {
            return Err(Self::poison_locked(
                &mut state,
                "Scribe resource release underflow",
            ));
        }
        let plan = self.plan();
        let prior_borrow = state
            .scribe_memory_used_bytes
            .saturating_sub(plan.scribe_floor_bytes);
        let next = state.scribe_memory_used_bytes - bytes;
        let next_borrow = next.saturating_sub(plan.scribe_floor_bytes);
        let released_elastic = prior_borrow - next_borrow;
        if state.elastic_memory_used_bytes < released_elastic {
            return Err(Self::poison_locked(
                &mut state,
                "Scribe elastic release underflow",
            ));
        }
        state.scribe_memory_used_bytes = next;
        state.elastic_memory_used_bytes -= released_elastic;
        Ok(())
    }

    /// Atomically acquires the complete currently-free Oracle query envelope.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Occupied`] when another Oracle query is
    /// active or the exact memory/scratch minimum cannot be owned. No counter is
    /// changed on refusal.
    pub(crate) fn try_acquire_oracle(
        &self,
        request: OracleResourceRequest,
    ) -> Result<OracleQueryResources, BifrostResourceError> {
        let mut state = self.lock_state()?;
        let plan = self.plan();
        if plan.oracle_floor_bytes == 0 {
            return Err(BifrostResourceError::InvalidPlan {
                detail: "Oracle resources requested while the role is inactive".to_owned(),
            });
        }
        if state.oracle_query_active {
            return Err(BifrostResourceError::Occupied {
                detail: "another Oracle query owns the pod resource envelope".to_owned(),
            });
        }
        let free_elastic = plan
            .elastic_memory_bytes
            .checked_sub(state.elastic_memory_used_bytes)
            .ok_or_else(|| Self::poison_locked(&mut state, "elastic memory underflow"))?;
        let memory_bytes = plan
            .oracle_floor_bytes
            .checked_add(free_elastic)
            .ok_or_else(accounting_overflow)?;
        let scratch_bytes = plan
            .scratch_limit_bytes
            .checked_sub(state.scratch_used_bytes)
            .ok_or_else(|| Self::poison_locked(&mut state, "scratch underflow"))?;
        if memory_bytes < ROLE_MEMORY_FLOOR_BYTES || scratch_bytes == 0 {
            return Err(BifrostResourceError::Occupied {
                detail: format!(
                    "Oracle requires at least {ROLE_MEMORY_FLOOR_BYTES} memory bytes and positive scratch"
                ),
            });
        }
        let target_partitions =
            oracle_target_partitions(plan.effective_cpu, request.local_ratio, memory_bytes)?;
        state.elastic_memory_used_bytes = state
            .elastic_memory_used_bytes
            .checked_add(free_elastic)
            .ok_or_else(accounting_overflow)?;
        state.scratch_used_bytes = state
            .scratch_used_bytes
            .checked_add(scratch_bytes)
            .ok_or_else(accounting_overflow)?;
        state.oracle_query_active = true;
        let memory_pool = bounded_memory_pool(memory_bytes);
        Ok(OracleQueryResources {
            memory_bytes,
            scratch_bytes,
            target_partitions,
            memory_pool,
            elastic_bytes: free_elastic,
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
        let next_memory = state
            .elastic_memory_used_bytes
            .checked_add(memory_bytes)
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
            return Err(BifrostResourceError::Occupied {
                detail: "Forge request exceeds currently free memory, scratch, or reader permits"
                    .to_owned(),
            });
        }
        state.elastic_memory_used_bytes = next_memory;
        state.scratch_used_bytes = next_scratch;
        state.forge_reader_permits_used = next_readers;
        Ok(ForgeRewriteResources {
            memory_bytes,
            scratch_bytes,
            reader_permits: usize::from(reader_permits),
            memory_pool: bounded_memory_pool(memory_bytes),
            governor: self.clone(),
            release_result: None,
        })
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, ResourceState>, BifrostResourceError> {
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

    fn poison_locked(state: &mut ResourceState, detail: &str) -> BifrostResourceError {
        state.poisoned = true;
        BifrostResourceError::Poisoned {
            detail: detail.to_owned(),
        }
    }

    /// Marks the shared allocator untrustworthy after coupled accounting fails.
    pub(crate) fn poison(&self, detail: &'static str) {
        if let Ok(mut state) = self.inner.state.lock() {
            state.poisoned = true;
            tracing::error!(detail, "Bifrost resource accounting poisoned");
        } else {
            tracing::error!(detail, "Bifrost resource lock poisoned");
        }
    }
}

/// Rollback owner for one provisional Scribe allocation.
pub(crate) struct ScribeResourceGrowth {
    bytes: usize,
    governor: BifrostResourceGovernor,
    committed: bool,
}

impl ScribeResourceGrowth {
    /// Commits root ownership after every coupled Scribe counter succeeds.
    pub(crate) fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for ScribeResourceGrowth {
    /// Rolls back root ownership when a later coupled allocation step fails.
    fn drop(&mut self) {
        if !self.committed
            && let Err(error) = self.governor.release_scribe(self.bytes)
        {
            tracing::error!(%error, "Scribe resource allocation rollback failed");
        }
    }
}

/// Query-lifetime Oracle memory, scratch, and adaptive parallelism owner.
#[derive(Debug)]
pub struct OracleQueryResources {
    /// Exact bounded memory available to all query consumers.
    pub memory_bytes: usize,
    /// Exact bounded disposable scratch capacity.
    pub scratch_bytes: u64,
    /// Query-local `DataFusion` target partition count.
    pub target_partitions: usize,
    /// One shared pool used by `DataFusion` and every query-owned Wyrd consumer.
    memory_pool: Arc<dyn MemoryPool>,
    elastic_bytes: usize,
    governor: BifrostResourceGovernor,
    released: bool,
}

impl OracleQueryResources {
    /// Builds the tracked first-come, first-served query-local `DataFusion` pool.
    #[must_use]
    pub fn memory_pool(&self) -> Arc<dyn MemoryPool> {
        Arc::clone(&self.memory_pool)
    }

    fn release(&mut self) -> Result<(), BifrostResourceError> {
        if self.released {
            return Ok(());
        }
        let mut state = self.governor.lock_state()?;
        if !state.oracle_query_active
            || state.elastic_memory_used_bytes < self.elastic_bytes
            || state.scratch_used_bytes < self.scratch_bytes
        {
            return Err(BifrostResourceGovernor::poison_locked(
                &mut state,
                "Oracle query release underflow",
            ));
        }
        state.elastic_memory_used_bytes -= self.elastic_bytes;
        state.scratch_used_bytes -= self.scratch_bytes;
        state.oracle_query_active = false;
        self.released = true;
        Ok(())
    }
}

/// Query-local ownership registered in the same pool as `DataFusion` operators.
///
/// The reservation is a nested accounting owner inside an already-admitted
/// pod envelope. It never charges the pod governor a second time.
#[derive(Debug)]
pub(crate) struct OracleQueryMemoryReservation {
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
    pub(crate) fn bytes(&self) -> usize {
        self.reservation.size()
    }

    /// Fails closed when telemetry and query-pool ownership diverge.
    pub(crate) fn poison(&self) {
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
        if state.elastic_memory_used_bytes < self.memory_bytes
            || state.scratch_used_bytes < self.scratch_bytes
            || state.forge_reader_permits_used < self.reader_permits
        {
            self.release_result = Some(ForgeResourceReleaseResult::Poisoned);
            return Err(BifrostResourceGovernor::poison_locked(
                &mut state,
                "Forge release underflow",
            ));
        }
        state.elastic_memory_used_bytes -= self.memory_bytes;
        state.scratch_used_bytes -= self.scratch_bytes;
        state.forge_reader_permits_used -= self.reader_permits;
        self.release_result = Some(ForgeResourceReleaseResult::Released);
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

redacted
///
/// # Errors
///
/// Returns [`BifrostResourceError::InvalidPlan`] for zero CPU, non-finite or
/// out-of-range locality, zero memory, or checked `cpu * 4` overflow.
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
    let memory = memory_bytes / ORACLE_PARTITION_MEMORY_BYTES;
    Ok(locality.min(memory).max(1))
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

fn accounting_overflow() -> BifrostResourceError {
    BifrostResourceError::Poisoned {
        detail: "resource accounting overflow".to_owned(),
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
            roles
                .scribe()
                .expect("Scribe capability must be enabled")
                .memory_governor()
                .pod_limit_bytes(),
            768 * MIB
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

    /// Enabled roles alone receive protected floors and elastic arithmetic is exact.
    #[test]
    fn resource_plan_reserves_only_enabled_role_floors() {
        let gib = 1024 * MIB;
        let cases = [
            (&[BifrostRole::Oracle][..], 0, 256 * MIB, 512 * MIB),
            (&[BifrostRole::Scribe][..], 256 * MIB, 0, 512 * MIB),
            (&[BifrostRole::Forge][..], 0, 0, 768 * MIB),
            (
                &[BifrostRole::Scribe, BifrostRole::Oracle][..],
                256 * MIB,
                256 * MIB,
                256 * MIB,
            ),
        ];
        for (roles, scribe, oracle, elastic) in cases {
            let governor = BifrostResourceGovernor::from_snapshot(snapshot(gib), policy(roles))
                .expect("resource plan must fit");
            let plan = governor.plan();
            assert_eq!(plan.managed_memory_bytes, 768 * MIB);
            assert_eq!(plan.scribe_floor_bytes, scribe);
            assert_eq!(plan.oracle_floor_bytes, oracle);
            assert_eq!(plan.elastic_memory_bytes, elastic);
        }
    }

    /// Memory and scratch ownership is one atomic Oracle grant and exact release.
    #[test]
    fn resource_grant_is_atomic_across_memory_and_scratch() {
        let governor = BifrostResourceGovernor::from_snapshot(
            snapshot(768 * MIB),
            policy(&[BifrostRole::Scribe, BifrostRole::Oracle]),
        )
        .expect("minimum combined plan must fit");
        let first = governor
            .try_acquire_oracle(OracleResourceRequest { local_ratio: 0.0 })
            .expect("first query owns the complete grant");
        assert!(
            governor
                .try_acquire_oracle(OracleResourceRequest { local_ratio: 1.0 })
                .is_err()
        );
        let occupied = governor.snapshot().expect("snapshot");
        assert!(occupied.oracle_query_active);
        drop(first);
        assert_eq!(
            governor.snapshot().expect("released snapshot"),
            ResourceSnapshot {
                plan: governor.plan(),
                scribe_memory_used_bytes: 0,
                elastic_memory_used_bytes: 0,
                scratch_used_bytes: 0,
                forge_reader_permits_used: 0,
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
        assert_eq!(
            oracle_target_partitions(8, 0.0, 256 * MIB).expect("bounded"),
            1
        );
        assert!(oracle_target_partitions(usize::MAX, 0.0, gib).is_err());
    }

    /// Portable source precedence selects the tightest injected host/cgroup bounds.
    #[test]
    fn resource_detector_resolves_portable_sources_in_order() {
        let mut injected = snapshot(2 * 1024 * MIB);
        injected.memory_limit_bytes = 1024 * MIB;
        injected.memory_source = ResourceSource::CgroupV2;
        injected.effective_cpu = 3;
        injected.cpu_source = ResourceSource::CgroupV1;
        let governor =
            BifrostResourceGovernor::from_snapshot(injected, policy(&[BifrostRole::Oracle]))
                .expect("injected portable sources must produce a plan");
        assert_eq!(governor.sources().memory, ResourceSource::CgroupV2);
        assert_eq!(governor.sources().cpu, ResourceSource::CgroupV1);
        assert_eq!(governor.sources().scratch, ResourceSource::Filesystem);
        assert_eq!(parse_cpuset("0-2,5"), Some(4));
        assert_eq!(parse_cpuset("4-2"), None);
    }

    /// Absolute overrides can tighten but never inflate detected resources.
    #[test]
    fn resource_detector_uses_tightest_host_cgroup_and_override_bound() {
        let mut policy = policy(&[BifrostRole::Oracle]);
        policy.memory_limit_bytes = Some(768 * MIB);
        policy.effective_cpu = Some(2);
        let governor = BifrostResourceGovernor::from_snapshot(snapshot(1024 * MIB), policy)
            .expect("reducing overrides must be accepted");
        assert_eq!(governor.plan().memory_limit_bytes, 768 * MIB);
        assert_eq!(governor.plan().effective_cpu, 2);
        assert_eq!(governor.sources().memory, ResourceSource::Override);
        assert_eq!(governor.sources().cpu, ResourceSource::Override);
    }

    /// Overrides cap one global plan and never create per-role silos.
    #[test]
    fn resource_detector_applies_absolute_overrides_without_role_silos() {
        let mut policy = policy(&[BifrostRole::Scribe, BifrostRole::Oracle]);
        policy.memory_limit_bytes = Some(768 * MIB);
        policy.scratch_limit_bytes = Some(512 * MIB as u64);
        let governor = BifrostResourceGovernor::from_snapshot(snapshot(1024 * MIB), policy)
            .expect("combined minimum must fit");
        assert_eq!(governor.plan().managed_memory_bytes, 512 * MIB);
        assert_eq!(governor.plan().elastic_memory_bytes, 0);
        assert_eq!(governor.plan().scratch_limit_bytes, 512 * MIB as u64);
        assert_eq!(governor.sources().scratch, ResourceSource::Override);
    }

    /// Minimum process and filesystem reserves fail closed before activation.
    #[test]
    fn resource_plan_enforces_minimum_viable_process_and_disk_floors() {
        assert!(
            BifrostResourceGovernor::from_snapshot(
                snapshot(512 * MIB - 1),
                policy(&[BifrostRole::Oracle]),
            )
            .is_err()
        );
        let mut insufficient_disk = snapshot(768 * MIB);
        insufficient_disk.scratch_available_bytes = MIN_SCRATCH_FREE_BYTES;
        assert!(
            BifrostResourceGovernor::from_snapshot(
                insufficient_disk,
                policy(&[BifrostRole::Oracle]),
            )
            .is_err()
        );
    }

    /// Enabled floors that exceed managed memory fail before any lease exists.
    #[test]
    fn resource_plan_rejects_floors_above_managed_memory_before_activation() {
        let error = BifrostResourceGovernor::from_snapshot(
            snapshot(768 * MIB - 1),
            policy(&[BifrostRole::Scribe, BifrostRole::Oracle]),
        )
        .expect_err("combined floors must not be weakened");
        assert!(matches!(error, BifrostResourceError::InvalidPlan { .. }));
    }

    /// A complete Oracle grant owns one finite greedy pool and releases exactly.
    #[test]
    fn oracle_runtime_pool_is_issued_by_query_lease() {
        let governor = BifrostResourceGovernor::from_snapshot(
            snapshot(1024 * MIB),
            policy(&[BifrostRole::Scribe, BifrostRole::Oracle]),
        )
        .expect("combined plan");
        let query = governor
            .try_acquire_oracle(OracleResourceRequest { local_ratio: 0.0 })
            .expect("complete query grant");
        assert_eq!(query.memory_bytes, 512 * MIB);
        let pool = query.memory_pool();
        let reservation = MemoryConsumer::new("oracle-test").register(&pool);
        reservation
            .try_grow(query.memory_bytes)
            .expect("aggregate grant is usable");
        assert!(reservation.try_grow(1).is_err());
        reservation.shrink(query.memory_bytes);
        assert_eq!(pool.reserved(), 0);
        drop(query);
        assert!(!governor.snapshot().expect("snapshot").oracle_query_active);
    }

    /// Forge's runtime pool remains nested in and bounded by its retained lease.
    #[test]
    fn forge_harness_pool_is_issued_by_operation_lease() {
        let governor = BifrostResourceGovernor::from_snapshot(
            snapshot(1024 * MIB),
            policy(&[BifrostRole::Forge]),
        )
        .expect("Forge-only plan");
        let lease = governor
            .try_acquire_forge(128 * MIB, 64 * MIB as u64, 1)
            .expect("Forge operation lease");
        let pool = lease.memory_pool();
        let reservation = MemoryConsumer::new("forge-operation-test").register(&pool);
        reservation
            .try_grow(128 * MIB)
            .expect("lease pool accepts exact capacity");
        assert!(reservation.try_grow(1).is_err());
        reservation.shrink(128 * MIB);
        drop(lease);
        let released = governor.snapshot().expect("released Forge snapshot");
        assert_eq!(released.elastic_memory_used_bytes, 0);
        assert_eq!(released.scratch_used_bytes, 0);
    }

    /// Live detection reaches the same checked constructor as an injection.
    ///
    /// `detect` may legitimately fail on a constrained CI host, so this asserts
    /// the delegation contract: whatever detection resolves, the result is a
    /// plan produced by the one policy stage, never a detection-specific path.
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
                assert_eq!(
                    BifrostRuntimeResources::from_snapshot(detected, policy)
                        .expect("the injected path accepts the detected observation")
                        .plan(),
                    runtime.plan(),
                    "detection must resolve through the same checked constructor"
                );
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
        assert_eq!(
            oracle
                .snapshot()
                .expect("Oracle observes the shared root")
                .elastic_memory_used_bytes,
            64 * MIB,
            "a Forge lease must be visible through every sibling capability"
        );
        drop(lease);
        assert_eq!(
            oracle
                .snapshot()
                .expect("released")
                .elastic_memory_used_bytes,
            0
        );
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
            .try_acquire_query(OracleResourceRequest { local_ratio: 0.0 })
            .expect("sole query owns the complete grant");
        let occupied = oracle.snapshot().expect("occupied snapshot");
        assert!(
            oracle
                .try_acquire_query(OracleResourceRequest { local_ratio: 0.0 })
                .is_err()
        );
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
            oracle
                .try_acquire_query(OracleResourceRequest { local_ratio: 0.0 })
                .is_ok(),
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
                    memory_bytes: plan.elastic_memory_bytes + 1,
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
            .try_acquire_query(OracleResourceRequest { local_ratio: 0.0 })
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
    #[test]
    fn scribe_floor_survives_oracle_and_forge_elastic_pressure() {
        let governor = BifrostResourceGovernor::from_snapshot(
            snapshot(1024 * MIB),
            policy(&[BifrostRole::Scribe, BifrostRole::Oracle, BifrostRole::Forge]),
        )
        .expect("combined role plan");
        let plan = governor.plan();
        assert_eq!(plan.scribe_floor_bytes, ROLE_MEMORY_FLOOR_BYTES);
        let memory =
            crate::scribe::memory::BifrostMemoryGovernor::from_resource_governor(governor.clone())
                .expect("Scribe global handle");
        let scribe_owner = memory
            .scribe_budget()
            .try_reserve_maintenance(crate::scribe::memory::MemoryCategory::Active, 300 * MIB)
            .expect("Scribe uses its floor and borrows elastic memory");
        let with_scribe = governor.snapshot().expect("Scribe ownership snapshot");
        assert_eq!(with_scribe.scribe_memory_used_bytes, 300 * MIB);
        assert_eq!(with_scribe.elastic_memory_used_bytes, 44 * MIB);
        let query = governor
            .try_acquire_oracle(OracleResourceRequest { local_ratio: 0.0 })
            .expect("Oracle owns only its floor and shared elastic memory");
        assert_eq!(query.memory_bytes, plan.oracle_floor_bytes + 212 * MIB);
        assert!(governor.try_acquire_forge(1, 1, 1).is_err());
        assert_eq!(governor.plan().scribe_floor_bytes, ROLE_MEMORY_FLOOR_BYTES);
        drop(query);
        drop(scribe_owner);
        let forge = governor
            .try_acquire_forge(plan.elastic_memory_bytes, plan.scratch_limit_bytes, 1)
            .expect("Forge may own all elastic resources after Oracle releases");
        assert_eq!(governor.plan().scribe_floor_bytes, ROLE_MEMORY_FLOOR_BYTES);
        drop(forge);
        let snapshot = governor.snapshot().expect("released resource snapshot");
        assert_eq!(snapshot.elastic_memory_used_bytes, 0);
        assert_eq!(snapshot.scratch_used_bytes, 0);
    }
}
