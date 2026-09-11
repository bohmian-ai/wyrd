//! Authoritative hot-source tracking for one Scribe seal key.
//!
//! A row admitted into Scribe is readable from exactly one place at a time, and
//! that place moves: first the in-memory generation that accepted it, then the
//! durable local sorted run staged from that generation, then the published hot
//! object in the catalog. Each move is a handover, not a copy — after it, the
//! previous holder may be released.
//!
//! Handovers are where a live-tail reader silently loses rows. Release the
//! memtable generation a moment early and a reader sees a gap; forget to
//! advance after publication and a reader keeps paying to scan a local file the
//! catalog already serves. This registry makes the handover explicit: every
//! generation has exactly one authority at every moment, transitions only ever
//! move forward, and a generation cannot be released until something else is
//! already authoritative for it.
//!
//! The registry tracks authority only. It performs no IO, reads no rows, and
//! never decides *when* to stage or publish; those belong to the staging and
//! publication owners that call it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::scribe::assembly::StagedMemberId;
use crate::scribe::seal_key::SealKey;
use crate::scribe::wal::WalLsn;

/// Stable identity of one Scribe generation within its seal key.
///
/// A seal key is served by every shard lane that routed rows for it, and each
/// lane numbers its own generations from one, so the generation number alone is
/// not an identity — two lanes would collide on it. The pair is, and it is the
/// same pair a staged member is named by.
///
/// Ordering is by generation first: a lower generation is older work regardless
/// of which lane produced it, which is what makes "read the oldest rows first"
/// and "authority only moves forward" checkable rather than merely intended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GenerationOrdinal {
    /// Generation number within the shard lane that froze it.
    generation: u64,
    /// Pod-local shard lane that froze the generation.
    shard: u16,
}

impl GenerationOrdinal {
    /// Builds an ordinal from the lane and the generation it numbered.
    #[must_use]
    pub const fn new(shard: u16, generation: u64) -> Self {
        Self { generation, shard }
    }

    /// Returns the generation number within its lane.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.generation
    }

    /// Returns the shard lane that froze the generation.
    #[must_use]
    pub const fn shard(self) -> u16 {
        self.shard
    }
}

/// Where one generation's rows are currently readable.
///
/// The order of the variants is the order of the lifecycle, and
/// [`HotAuthority::stage`] turns that into a number a transition can compare
/// against, so adding a stage without deciding where it sits in the lifecycle
/// is not possible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotAuthority {
    /// Readable from the in-memory generation that accepted the rows.
    Memtable,
    /// Readable from durable local sorted runs fsynced under the staging root.
    ///
    /// Reaching this state is the first durable boundary: the rows survive a
    /// process restart without the WAL, which is exactly what permits the WAL
    /// segments behind them to be retired.
    ///
    /// The runs are carried here rather than looked up per read because this is
    /// the authority a live-tail reader resolves: asking the staged namespace
    /// again on every query would put a directory scan on the read path and
    /// could answer with a member this registry has already handed over.
    StagedRun {
        /// Staged member identity serving the rows, for read provenance.
        member: StagedMemberId,
        /// Fsynced local run paths serving the rows, in merge order.
        runs: Vec<PathBuf>,
        /// Encoded bytes the runs occupy on the staging volume.
        bytes: u64,
        /// Inclusive WAL bounds the member covers.
        wal: (WalLsn, WalLsn),
    },
    /// Readable from the published hot objects recorded in the catalog.
    Published {
        /// Every remote object key the generation's catalog rows point at, in
        /// publication order. A claim that packs its merged rows into more than
        /// one row group writes more than one object, and naming only the first
        /// would leave the rest of the generation unattributed.
        object_keys: Vec<String>,
    },
}

impl HotAuthority {
    /// Returns this authority's position in the hot lifecycle.
    ///
    /// Used only to compare two authorities for direction. It is deliberately
    /// not public: callers should ask whether a transition is legal, not
    /// reimplement the ordering.
    const fn stage(&self) -> u8 {
        match self {
            Self::Memtable => 0,
            Self::StagedRun { .. } => 1,
            Self::Published { .. } => 2,
        }
    }

    /// Returns the fixed-cardinality label naming this authority.
    ///
    /// Safe as a metric label: it carries no tenant, table, path, or key.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Memtable => "memtable",
            Self::StagedRun { .. } => "staged_run",
            Self::Published { .. } => "published",
        }
    }

    /// Reports whether rows under this authority survive a process restart.
    ///
    /// Only a durable authority permits releasing the WAL segments behind the
    /// generation, so this is the predicate the retirement boundary asks.
    #[must_use]
    pub const fn is_durable(&self) -> bool {
        matches!(self, Self::StagedRun { .. } | Self::Published { .. })
    }
}

/// Why a hot-source transition was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HotSourceError {
    /// A transition tried to move authority backwards in the lifecycle.
    #[error(
        "Scribe hot authority for generation {generation} cannot move backwards from `{from}` to `{to}`"
    )]
    Backwards {
        /// Generation the transition targeted.
        generation: u64,
        /// Current authority label.
        from: &'static str,
        /// Refused authority label.
        to: &'static str,
    },
    /// A replay named the same lifecycle stage with different durable identity.
    #[error(
        "Scribe hot authority for generation {generation} contradicts the recorded `{authority}` identity"
    )]
    Contradictory {
        /// Generation whose replay identity disagreed.
        generation: u64,
        /// Lifecycle stage carrying the contradictory identity.
        authority: &'static str,
    },
    /// A transition or release named a generation with no recorded authority.
    #[error("Scribe hot authority for generation {generation} is not registered")]
    Unregistered {
        /// Generation the caller named.
        generation: u64,
    },
    /// A generation was registered twice.
    #[error("Scribe hot authority for generation {generation} is already registered")]
    AlreadyRegistered {
        /// Generation the caller tried to register again.
        generation: u64,
    },
    /// Releasing was refused because nothing durable holds the rows yet.
    #[error(
        "Scribe cannot release generation {generation}: its rows are still only in `{authority}`"
    )]
    NotDurable {
        /// Generation the caller tried to release.
        generation: u64,
        /// Current, non-durable authority label.
        authority: &'static str,
    },
    /// The registry lock was poisoned by a panicking holder.
    #[error("Scribe hot-source registry lock poisoned: {detail}")]
    Poisoned {
        /// Lock failure detail.
        detail: String,
    },
}

/// One staged member a live-tail read may serve rows from.
///
/// Carries the provenance a returned batch needs — which member produced the
/// rows and which WAL positions it covers — so a caller can suppress a member
/// a pinned cut already owns without reopening the staged namespace.
#[derive(Debug, Clone)]
pub struct StagedSource {
    /// Seal key whose partition the runs belong to.
    pub key: SealKey,
    /// Generation the member was staged from.
    pub generation: GenerationOrdinal,
    /// Staged member identity serving the rows.
    pub member: StagedMemberId,
    /// Fsynced local run paths, in merge order.
    pub runs: Vec<PathBuf>,
    /// Encoded bytes the runs occupy.
    pub bytes: u64,
    /// Inclusive WAL bounds the member covers.
    pub wal: (WalLsn, WalLsn),
}

/// One generation's current authority, as the registry holds it.
///
/// Read-only observation used to prove which of a pod's live generations are
/// already served by a published object and which are still served by their
/// memtable or staged runs.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone)]
pub struct LiveAuthority {
    /// Seal key the generation belongs to.
    pub key: SealKey,
    /// Generation the authority describes.
    pub generation: GenerationOrdinal,
    /// Where the generation's rows are readable right now.
    pub authority: HotAuthority,
}

/// Staged sources one reader holds open for the length of its read.
///
/// The lease is what stops cleanup from deleting a member's runs between the
/// moment a reader resolved it and the moment the reader opens its files. It
/// releases on drop, including on an early return or a panic, so a refused read
/// never strands a member's directory.
#[derive(Debug)]
pub struct StagedSourceLease {
    /// Registry the leases are released back to.
    registry: std::sync::Arc<ScribeHotSourceRegistry>,
    /// One entry per lease taken, in the order they were taken.
    held: Vec<(SealKey, GenerationOrdinal)>,
    /// Staged sources this lease keeps readable, oldest first.
    sources: Vec<StagedSource>,
}

impl StagedSourceLease {
    /// Returns the leased staged sources, oldest first.
    #[must_use]
    pub fn sources(&self) -> &[StagedSource] {
        &self.sources
    }
}

impl Drop for StagedSourceLease {
    /// Releases every lease this read held.
    fn drop(&mut self) {
        self.registry.release_leases(&self.held);
    }
}

/// Live authority for one seal key's generations.
#[derive(Debug, Default)]
struct KeyAuthorities {
    /// Current authority per registered generation.
    by_generation: HashMap<GenerationOrdinal, HotAuthority>,
    /// Readers currently holding each generation's staged runs open.
    leases: HashMap<GenerationOrdinal, usize>,
}

/// Pod-wide registry of where every live generation's rows are readable.
#[derive(Debug, Default)]
pub struct ScribeHotSourceRegistry {
    /// Authorities per seal key.
    state: Mutex<HashMap<SealKey, KeyAuthorities>>,
    /// Signalled whenever a lease is released, so cleanup can wait on it.
    released: tokio::sync::Notify,
}

impl ScribeHotSourceRegistry {
    /// Builds an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a new generation as readable from the memtable.
    ///
    /// Every generation enters the lifecycle here. Registering twice is refused
    /// rather than ignored: a second registration would silently reset an
    /// already-advanced authority back to the memtable, and a reader following
    /// that would look for rows in a generation that had already been released.
    ///
    /// # Errors
    ///
    /// Returns [`HotSourceError::AlreadyRegistered`] when the generation is
    /// already tracked, and [`HotSourceError::Poisoned`] on a poisoned lock.
    pub fn register_memtable(
        &self,
        key: &SealKey,
        generation: GenerationOrdinal,
    ) -> Result<(), HotSourceError> {
        let mut state = self.lock()?;
        let authorities = state.entry(key.clone()).or_default();
        if authorities.by_generation.contains_key(&generation) {
            return Err(HotSourceError::AlreadyRegistered {
                generation: generation.get(),
            });
        }
        authorities
            .by_generation
            .insert(generation, HotAuthority::Memtable);
        Ok(())
    }

    /// Advances one generation's authority to a later lifecycle stage.
    ///
    /// The new authority must sit strictly later in the lifecycle than the
    /// current one. Re-advancing to the same stage is refused too: two staged
    /// runs for one generation would mean two answers to "where are these rows",
    /// and nothing in the registry could say which is the live one.
    ///
    /// # Errors
    ///
    /// Returns [`HotSourceError::Unregistered`] when the generation is unknown,
    /// [`HotSourceError::Backwards`] when the move is not strictly forward, and
    /// [`HotSourceError::Poisoned`] on a poisoned lock.
    pub fn advance(
        &self,
        key: &SealKey,
        generation: GenerationOrdinal,
        next: HotAuthority,
    ) -> Result<(), HotSourceError> {
        let mut state = self.lock()?;
        let current = state
            .get_mut(key)
            .and_then(|authorities| authorities.by_generation.get_mut(&generation))
            .ok_or(HotSourceError::Unregistered {
                generation: generation.get(),
            })?;
        if next.stage() <= current.stage() {
            return Err(HotSourceError::Backwards {
                generation: generation.get(),
                from: current.label(),
                to: next.label(),
            });
        }
        *current = next;
        Ok(())
    }

    /// Atomically advances every named generation under one seal key.
    ///
    /// The registry prevalidates the complete set while holding its one state
    /// lock, then applies every move. An exact replay is idempotent; a missing
    /// generation, backwards move, or same-stage identity contradiction leaves
    /// the complete set unchanged.
    ///
    /// # Errors
    ///
    /// Returns a typed hot-source error when any member cannot make the exact
    /// requested transition, or when the registry lock is poisoned.
    pub fn advance_all_atomic(
        &self,
        key: &SealKey,
        transitions: &[(GenerationOrdinal, HotAuthority)],
    ) -> Result<(), HotSourceError> {
        let mut state = self.lock()?;
        let authorities = state.get_mut(key).ok_or_else(|| {
            transitions.first().map_or(
                HotSourceError::Unregistered { generation: 0 },
                |(generation, _)| HotSourceError::Unregistered {
                    generation: generation.get(),
                },
            )
        })?;
        for (generation, next) in transitions {
            let current =
                authorities
                    .by_generation
                    .get(generation)
                    .ok_or(HotSourceError::Unregistered {
                        generation: generation.get(),
                    })?;
            if current == next {
                continue;
            }
            if next.stage() == current.stage() {
                return Err(HotSourceError::Contradictory {
                    generation: generation.get(),
                    authority: current.label(),
                });
            }
            if next.stage() < current.stage() {
                return Err(HotSourceError::Backwards {
                    generation: generation.get(),
                    from: current.label(),
                    to: next.label(),
                });
            }
        }
        for (generation, next) in transitions {
            if let Some(current) = authorities.by_generation.get_mut(generation)
                && current != next
            {
                *current = next.clone();
            }
        }
        Ok(())
    }

    /// Releases one generation once something durable holds its rows.
    ///
    /// This is the read-side half of the WAL-retirement boundary: a generation
    /// may only leave the registry after its rows are readable from a durable
    /// authority, so a reader that resolved the key an instant before the
    /// release still finds those rows somewhere.
    ///
    /// # Errors
    ///
    /// Returns [`HotSourceError::Unregistered`] when the generation is unknown,
    /// [`HotSourceError::NotDurable`] when only the memtable holds the rows, and
    /// [`HotSourceError::Poisoned`] on a poisoned lock.
    pub fn release(
        &self,
        key: &SealKey,
        generation: GenerationOrdinal,
    ) -> Result<HotAuthority, HotSourceError> {
        let mut state = self.lock()?;
        let authorities = state.get_mut(key).ok_or(HotSourceError::Unregistered {
            generation: generation.get(),
        })?;
        let current =
            authorities
                .by_generation
                .get(&generation)
                .ok_or(HotSourceError::Unregistered {
                    generation: generation.get(),
                })?;
        if !current.is_durable() {
            return Err(HotSourceError::NotDurable {
                generation: generation.get(),
                authority: current.label(),
            });
        }
        let released = authorities
            .by_generation
            .remove(&generation)
            .unwrap_or(HotAuthority::Memtable);
        if authorities.by_generation.is_empty() {
            state.remove(key);
        }
        Ok(released)
    }

    /// Restores one generation's durable authority found by startup recovery.
    ///
    /// A restarted pod never held these rows in memory, so there is no memtable
    /// moment to register and then advance from. The durable evidence on the
    /// volume is the whole history the new process has, and this installs
    /// exactly that. Only a durable authority may be restored: a memtable
    /// authority would claim rows this process does not hold.
    ///
    /// # Errors
    ///
    /// Returns [`HotSourceError::AlreadyRegistered`] when the generation is
    /// already tracked, [`HotSourceError::NotDurable`] when the restored
    /// authority is the memtable, and [`HotSourceError::Poisoned`] on a
    /// poisoned lock.
    pub fn restore_durable(
        &self,
        key: &SealKey,
        generation: GenerationOrdinal,
        authority: HotAuthority,
    ) -> Result<(), HotSourceError> {
        if !authority.is_durable() {
            return Err(HotSourceError::NotDurable {
                generation: generation.get(),
                authority: authority.label(),
            });
        }
        let mut state = self.lock()?;
        let authorities = state.entry(key.clone()).or_default();
        if authorities.by_generation.contains_key(&generation) {
            return Err(HotSourceError::AlreadyRegistered {
                generation: generation.get(),
            });
        }
        authorities.by_generation.insert(generation, authority);
        Ok(())
    }

    /// Forgets a generation that was abandoned before becoming durable.
    ///
    /// Replay can admit a generation into the memtable and then refuse it when
    /// memory admission rejects the chunk. Nothing durable ever held those
    /// rows, so [`Self::release`] would rightly refuse them; the WAL is still
    /// their authority and the registry must simply stop tracking a generation
    /// that no longer exists.
    ///
    /// Forgetting an unknown generation is not an error: the caller is
    /// asserting the generation is gone, which it already is.
    ///
    /// # Errors
    ///
    /// Returns [`HotSourceError::Poisoned`] on a poisoned lock.
    pub fn discard(
        &self,
        key: &SealKey,
        generation: GenerationOrdinal,
    ) -> Result<(), HotSourceError> {
        let mut state = self.lock()?;
        let Some(authorities) = state.get_mut(key) else {
            return Ok(());
        };
        authorities.by_generation.remove(&generation);
        if authorities.by_generation.is_empty() {
            state.remove(key);
        }
        Ok(())
    }

    /// Returns one generation's current authority, if it is still registered.
    ///
    /// # Errors
    ///
    /// Returns [`HotSourceError::Poisoned`] on a poisoned lock.
    pub fn authority(
        &self,
        key: &SealKey,
        generation: GenerationOrdinal,
    ) -> Result<Option<HotAuthority>, HotSourceError> {
        Ok(self
            .lock()?
            .get(key)
            .and_then(|authorities| authorities.by_generation.get(&generation).cloned()))
    }

    /// Returns one key's live generations, oldest first, with their authorities.
    ///
    /// Ordered because a reader must scan generations in age order to produce
    /// rows in a defensible sequence, and the registry is the only place that
    /// knows the full live set for a key.
    ///
    /// # Errors
    ///
    /// Returns [`HotSourceError::Poisoned`] on a poisoned lock.
    pub fn live_generations(
        &self,
        key: &SealKey,
    ) -> Result<Vec<(GenerationOrdinal, HotAuthority)>, HotSourceError> {
        let state = self.lock()?;
        let Some(authorities) = state.get(key) else {
            return Ok(Vec::new());
        };
        let mut live: Vec<(GenerationOrdinal, HotAuthority)> = authorities
            .by_generation
            .iter()
            .map(|(generation, authority)| (*generation, authority.clone()))
            .collect();
        live.sort_by_key(|(generation, _)| *generation);
        Ok(live)
    }

    /// Returns every generation the registry still tracks, with its authority.
    ///
    /// The registry is the pod's single answer to "where are this generation's
    /// rows readable now", so a harness that has to prove one partition is
    /// served by a published object while a neighbouring partition is still
    /// served by a live authority reads that fact here rather than inferring it
    /// from object counts or WAL bounds.
    ///
    /// # Errors
    ///
    /// Returns [`HotSourceError::Poisoned`] on a poisoned lock.
    #[cfg(any(test, feature = "test-support"))]
    pub fn live_authorities_for_test(&self) -> Result<Vec<LiveAuthority>, HotSourceError> {
        let state = self.lock()?;
        let mut live: Vec<LiveAuthority> = state
            .iter()
            .flat_map(|(key, authorities)| {
                authorities
                    .by_generation
                    .iter()
                    .map(|(generation, authority)| LiveAuthority {
                        key: key.clone(),
                        generation: *generation,
                        authority: authority.clone(),
                    })
            })
            .collect();
        live.sort_by(|left, right| {
            (&left.key.partition, left.generation).cmp(&(&right.key.partition, right.generation))
        });
        Ok(live)
    }

    /// Returns the highest generation number one lane already owns for a key.
    ///
    /// Generation numbers are allocated by the shard lane that freezes them and
    /// start again at one in a new process, while startup recovery restores the
    /// authorities of members an earlier process numbered. A lane that begins
    /// numbering from one after such a restore would reuse an ordinal the
    /// registry still holds, and the registration that discovers it fails the
    /// pod's recovery. Asking the registry for the ordinal already in use is
    /// what lets the lane resume above it instead.
    ///
    /// Returns zero when the lane owns no generation for the key, so the first
    /// allocation is one.
    ///
    /// # Errors
    ///
    /// Returns [`HotSourceError::Poisoned`] on a poisoned lock.
    pub fn highest_generation(&self, key: &SealKey, shard: u16) -> Result<u64, HotSourceError> {
        let state = self.lock()?;
        Ok(state.get(key).map_or(0, |authorities| {
            authorities
                .by_generation
                .keys()
                .filter(|generation| generation.shard() == shard)
                .map(|generation| generation.get())
                .max()
                .unwrap_or(0)
        }))
    }

    /// Returns tenant keys whose staged runs still own readable rows.
    ///
    /// # Errors
    /// Returns [`HotSourceError::Poisoned`] on a poisoned lock.
    pub(crate) fn staged_seal_keys_for_tenant(
        &self,
        tenant: wyrd_spec::ids::DataTenantId,
    ) -> Result<Vec<SealKey>, HotSourceError> {
        Ok(self
            .lock()?
            .iter()
            .filter(|(key, authorities)| {
                key.tenant == tenant
                    && authorities
                        .by_generation
                        .values()
                        .any(|authority| matches!(authority, HotAuthority::StagedRun { .. }))
            })
            .map(|(key, _)| key.clone())
            .collect())
    }

    /// Returns every staged member readable for one table in a partition range.
    ///
    /// This is the read side of the staged boundary: a generation whose Arrow
    /// has been released is served from exactly these runs until its claim
    /// publishes. Results are ordered by partition and then generation so a
    /// reader scans rows in age order, and a generation that is still memtable
    /// or already published contributes nothing here — one authority per
    /// generation, never two.
    ///
    /// # Errors
    ///
    /// Returns [`HotSourceError::Poisoned`] on a poisoned lock.
    pub fn staged_sources(
        self: &std::sync::Arc<Self>,
        tenant: wyrd_spec::ids::DataTenantId,
        table: &crate::catalog::TableRef,
        start: crate::catalog::layout::TimePartition,
        end: crate::catalog::layout::TimePartition,
    ) -> Result<StagedSourceLease, HotSourceError> {
        let mut state = self.lock()?;
        let mut sources = Vec::new();
        let mut held = Vec::new();
        for (key, authorities) in state.iter() {
            if key.tenant != tenant || key.table != *table {
                continue;
            }
            if key.partition < start || key.partition > end {
                continue;
            }
            for (generation, authority) in &authorities.by_generation {
                let HotAuthority::StagedRun {
                    member,
                    runs,
                    bytes,
                    wal,
                } = authority
                else {
                    continue;
                };
                held.push((key.clone(), *generation));
                sources.push(StagedSource {
                    key: key.clone(),
                    generation: *generation,
                    member: *member,
                    runs: runs.clone(),
                    bytes: *bytes,
                    wal: *wal,
                });
            }
        }
        for (key, generation) in &held {
            let authorities = state.entry(key.clone()).or_default();
            let count = authorities.leases.entry(*generation).or_default();
            *count = count.saturating_add(1);
        }
        drop(state);
        sources.sort_by(|left, right| {
            left.key
                .partition
                .cmp(&right.key.partition)
                .then(left.generation.cmp(&right.generation))
        });
        Ok(StagedSourceLease {
            registry: std::sync::Arc::clone(self),
            held,
            sources,
        })
    }

    /// Waits until no reader still holds one generation's staged runs open.
    ///
    /// Cleanup calls this before deleting a member's directory. The reader
    /// opens each run lazily, so deleting under a live lease would turn an
    /// in-flight exact read into a refusal even though the rows are published
    /// and correct. Publication has already advanced the generation's authority
    /// by the time cleanup runs, so no new lease can be taken here and the wait
    /// is bounded by the reads that were already in flight.
    ///
    /// # Errors
    ///
    /// Returns [`HotSourceError::Poisoned`] on a poisoned lock.
    ///
    /// # Cancellation
    ///
    /// Cancelling the wait leaves the leases untouched; the member's directory
    /// simply is not removed yet, and the next cleanup attempt waits again.
    pub async fn drain_leases(
        &self,
        key: &SealKey,
        generation: GenerationOrdinal,
    ) -> Result<(), HotSourceError> {
        loop {
            let released = self.released.notified();
            if self.leases(key, generation)? == 0 {
                return Ok(());
            }
            released.await;
        }
    }

    /// Returns how many readers currently hold one generation's runs open.
    ///
    /// # Errors
    ///
    /// Returns [`HotSourceError::Poisoned`] on a poisoned lock.
    pub fn leases(
        &self,
        key: &SealKey,
        generation: GenerationOrdinal,
    ) -> Result<usize, HotSourceError> {
        Ok(self
            .lock()?
            .get(key)
            .and_then(|authorities| authorities.leases.get(&generation).copied())
            .unwrap_or(0))
    }

    /// Releases one held lease per entry, waking any cleanup waiting on it.
    fn release_leases(&self, held: &[(SealKey, GenerationOrdinal)]) {
        if let Ok(mut state) = self.state.lock() {
            for (key, generation) in held {
                let Some(authorities) = state.get_mut(key) else {
                    continue;
                };
                if let Some(count) = authorities.leases.get_mut(generation) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        authorities.leases.remove(generation);
                    }
                }
            }
        }
        self.released.notify_waiters();
    }

    /// Reports whether every generation of a key is durably held.
    ///
    /// The WAL segments behind a key may only retire when this is true: one
    /// memtable-only generation is enough to make the WAL the sole copy of
    /// those rows.
    ///
    /// # Errors
    ///
    /// Returns [`HotSourceError::Poisoned`] on a poisoned lock.
    pub fn is_fully_durable(&self, key: &SealKey) -> Result<bool, HotSourceError> {
        let state = self.lock()?;
        Ok(state.get(key).is_none_or(|authorities| {
            authorities
                .by_generation
                .values()
                .all(HotAuthority::is_durable)
        }))
    }

    /// Locks the registry, converting a poisoned lock into a typed error.
    ///
    /// # Errors
    ///
    /// Returns [`HotSourceError::Poisoned`] when a previous holder panicked.
    fn lock(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, HashMap<SealKey, KeyAuthorities>>, HotSourceError> {
        self.state.lock().map_err(|error| HotSourceError::Poisoned {
            detail: error.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{TableRef, TimeGranularity, TimePartition};
    use crate::namespaces::BifrostNamespace;
    use wyrd_spec::ids::DataTenantId;

    /// Builds one seal key for the registry fixtures.
    ///
    /// # Panics
    ///
    /// Panics when the fixed partition literal is not a valid partition, which
    /// would be a fixture bug rather than registry behavior.
    fn seal_key() -> SealKey {
        SealKey::new(
            DataTenantId::new_v7(),
            TableRef::new(BifrostNamespace::Bifrost, "events"),
            TimePartition::new(
                TimeGranularity::Day,
                chrono::DateTime::from_timestamp(1_772_150_400, 0)
                    .expect("a fixed representable instant"),
            )
            .expect("a fixed day boundary"),
        )
    }

    /// Builds a staged-run authority with a synthetic path.
    fn staged() -> HotAuthority {
        staged_at(PathBuf::from("/stage/run-1.parquet"))
    }

    /// Builds a staged-run authority naming one specific run path.
    ///
    /// A lease only names run paths, so a test that must prove the run is still
    /// readable under the lease supplies a real file here instead of the
    /// synthetic path the ordering fixtures use.
    fn staged_at(run: PathBuf) -> HotAuthority {
        HotAuthority::StagedRun {
            member: StagedMemberId::new(0, 1),
            runs: vec![run],
            bytes: 1_024,
            wal: (WalLsn::new(1), WalLsn::new(9)),
        }
    }

    /// Authority walks memtable to staged run to published, never backwards.
    ///
    /// # Panics
    ///
    /// Panics when a backwards or same-stage transition is accepted, which
    /// would leave two answers to where a generation's rows live.
    #[test]
    fn hot_authority_only_ever_moves_forward() {
        let registry = ScribeHotSourceRegistry::new();
        let key = seal_key();
        let generation = GenerationOrdinal::new(0, 1);
        registry
            .register_memtable(&key, generation)
            .expect("a new generation registers");
        assert_eq!(
            registry.authority(&key, generation).expect("locked"),
            Some(HotAuthority::Memtable)
        );

        registry
            .advance(&key, generation, staged())
            .expect("memtable advances to a staged run");
        let published = HotAuthority::Published {
            object_keys: vec!["hot/events/0001.parquet".to_owned()],
        };
        registry
            .advance(&key, generation, published.clone())
            .expect("a staged run advances to published");
        assert_eq!(
            registry.authority(&key, generation).expect("locked"),
            Some(published)
        );

        let backwards = registry
            .advance(&key, generation, staged())
            .expect_err("published never falls back to a staged run");
        assert!(matches!(
            backwards,
            HotSourceError::Backwards {
                from: "published",
                to: "staged_run",
                ..
            }
        ));

        // Re-registering would reset an advanced authority to the memtable.
        assert!(matches!(
            registry.register_memtable(&key, generation),
            Err(HotSourceError::AlreadyRegistered { .. })
        ));
    }

    /// A complete claim transition is all-or-none and exact replay is success.
    ///
    /// # Panics
    ///
    /// Panics when prevalidation mutates an earlier member, when the corrected
    /// complete retry does not publish both members, or when exact replay is
    /// rejected.
    #[test]
    fn complete_claim_authority_transition_is_atomic_and_idempotent() {
        let registry = ScribeHotSourceRegistry::new();
        let key = seal_key();
        let first = GenerationOrdinal::new(3, 1);
        let second = GenerationOrdinal::new(7, 2);
        registry
            .restore_durable(&key, first, staged())
            .expect("first staged member restores");
        let published = HotAuthority::Published {
            object_keys: vec!["hot/events/claim.parquet".to_owned()],
        };
        let transitions = vec![(first, published.clone()), (second, published.clone())];

        assert!(matches!(
            registry.advance_all_atomic(&key, &transitions),
            Err(HotSourceError::Unregistered { generation: 2 })
        ));
        assert_eq!(
            registry
                .authority(&key, first)
                .expect("first authority reads"),
            Some(staged()),
            "failure on a later member must not mutate an earlier member"
        );

        registry
            .restore_durable(&key, second, staged())
            .expect("missing staged member restores under the same identity");
        registry
            .advance_all_atomic(&key, &transitions)
            .expect("the complete retry publishes atomically");
        registry
            .advance_all_atomic(&key, &transitions)
            .expect("exact replay is idempotent success");
        for generation in [first, second] {
            assert_eq!(
                registry
                    .authority(&key, generation)
                    .expect("published authority reads"),
                Some(published.clone())
            );
        }
    }

    /// A generation is only released once something durable holds its rows.
    ///
    /// # Panics
    ///
    /// Panics when a memtable-only generation may be released, which is exactly
    /// the handover gap a live-tail reader would fall into.
    #[test]
    fn memtable_only_generation_cannot_be_released() {
        let registry = ScribeHotSourceRegistry::new();
        let key = seal_key();
        let generation = GenerationOrdinal::new(0, 7);
        registry
            .register_memtable(&key, generation)
            .expect("registers");

        let refusal = registry
            .release(&key, generation)
            .expect_err("nothing durable holds these rows yet");
        assert!(matches!(
            refusal,
            HotSourceError::NotDurable {
                authority: "memtable",
                ..
            }
        ));
        assert!(
            !registry.is_fully_durable(&key).expect("locked"),
            "a memtable-only generation keeps the WAL as the sole durable copy"
        );

        registry
            .advance(&key, generation, staged())
            .expect("staging succeeds");
        assert!(registry.is_fully_durable(&key).expect("locked"));
        let released = registry
            .release(&key, generation)
            .expect("release succeeds");
        assert!(released.is_durable());
        assert_eq!(registry.authority(&key, generation).expect("locked"), None);
    }

    /// One memtable-only generation blocks retirement for its whole key.
    ///
    /// # Panics
    ///
    /// Panics when a key with a mix of durable and memtable-only generations
    /// reports itself fully durable, or when live generations are not ordered
    /// oldest first.
    #[test]
    fn key_durability_and_ordering_span_every_live_generation() {
        let registry = ScribeHotSourceRegistry::new();
        let key = seal_key();
        for ordinal in [3_u64, 1, 2] {
            registry
                .register_memtable(&key, GenerationOrdinal::new(0, ordinal))
                .expect("registers");
        }
        registry
            .advance(&key, GenerationOrdinal::new(0, 1), staged())
            .expect("stages the oldest");
        registry
            .advance(&key, GenerationOrdinal::new(0, 3), staged())
            .expect("stages the newest");
        assert!(
            !registry.is_fully_durable(&key).expect("locked"),
            "generation 2 is still memtable-only"
        );

        let live = registry.live_generations(&key).expect("locked");
        let ordinals: Vec<u64> = live
            .iter()
            .map(|(generation, _)| generation.get())
            .collect();
        assert_eq!(ordinals, vec![1, 2, 3], "readers scan oldest first");
        assert_eq!(live[1].1.label(), "memtable");

        registry
            .advance(&key, GenerationOrdinal::new(0, 2), staged())
            .expect("stages the last one");
        assert!(registry.is_fully_durable(&key).expect("locked"));
    }

    /// Transitions against an unregistered generation are refused.
    ///
    /// # Panics
    ///
    /// Panics when advancing or releasing an unknown generation silently
    /// succeeds, which would let a caller believe a handover happened.
    #[test]
    fn unregistered_generations_are_refused() {
        let registry = ScribeHotSourceRegistry::new();
        let key = seal_key();
        let generation = GenerationOrdinal::new(0, 9);
        assert!(matches!(
            registry.advance(&key, generation, staged()),
            Err(HotSourceError::Unregistered { generation: 9 })
        ));
        assert!(matches!(
            registry.release(&key, generation),
            Err(HotSourceError::Unregistered { generation: 9 })
        ));
        assert_eq!(registry.authority(&key, generation).expect("locked"), None);
        assert!(
            registry.is_fully_durable(&key).expect("locked"),
            "a key with no live generations blocks nothing"
        );
    }

    /// A staged read sees exactly the generations the staged boundary owns, in
    /// age order, scoped to the requested table and partition range.
    ///
    /// # Panics
    ///
    /// Panics when a fixture registration or transition is refused.
    #[test]
    fn staged_sources_are_scoped_ordered_and_exclude_other_authorities() {
        let registry = std::sync::Arc::new(ScribeHotSourceRegistry::new());
        let key = seal_key();
        let next_day = SealKey::new(
            key.tenant,
            key.table.clone(),
            TimePartition::new(
                TimeGranularity::Day,
                chrono::DateTime::from_timestamp(1_772_236_800, 0)
                    .expect("a fixed representable instant"),
            )
            .expect("a fixed day boundary"),
        );
        let other_table = SealKey::new(
            key.tenant,
            TableRef::new(BifrostNamespace::Bifrost, "other"),
            key.partition,
        );
        let other_tenant = SealKey::new(DataTenantId::new_v7(), key.table.clone(), key.partition);
        for (key, generation) in [
            (&key, 2_u64),
            (&key, 1),
            (&next_day, 3),
            (&other_table, 4),
            (&other_tenant, 7),
        ] {
            let generation = GenerationOrdinal::new(0, generation);
            registry
                .register_memtable(key, generation)
                .expect("generation registers");
            registry
                .advance(key, generation, staged())
                .expect("generation stages");
        }
        let memtable_only = GenerationOrdinal::new(0, 5);
        registry
            .register_memtable(&key, memtable_only)
            .expect("generation registers");
        let published = GenerationOrdinal::new(0, 6);
        registry
            .register_memtable(&key, published)
            .expect("generation registers");
        registry
            .advance(&key, published, staged())
            .expect("generation stages");
        registry
            .advance(
                &key,
                published,
                HotAuthority::Published {
                    object_keys: vec!["objects/one.parquet".to_owned()],
                },
            )
            .expect("generation publishes");

        let sources = registry
            .staged_sources(key.tenant, &key.table, key.partition, next_day.partition)
            .expect("locked");
        let named: Vec<(TimePartition, u64)> = sources
            .sources()
            .iter()
            .map(|source| (source.key.partition, source.generation.get()))
            .collect();
        assert_eq!(
            named,
            vec![
                (key.partition, 1),
                (key.partition, 2),
                (next_day.partition, 3),
            ],
            "only staged generations of the requested table and range are readable, oldest first"
        );
    }

    /// A resolved staged read leases its members until the reader drops them.
    ///
    /// Cleanup deletes a member's runs as soon as its claim publishes, and the
    /// reader opens those runs lazily. Without the lease a read that had already
    /// resolved a member would refuse on a file that vanished under it, so the
    /// lease is what makes "no deletion under a pinned reader" true rather than
    /// merely likely.
    ///
    /// Two concurrent readers prove the ownership rule the count depends on:
    /// each lease releases exactly once on drop, so cleanup waits for the last
    /// reader rather than the first, and a settled generation cannot be
    /// released a second time by a later drain. The member names a real staged
    /// run, and both readers open it while cleanup is already blocked, so the
    /// lease is proven to keep the source readable rather than only counted.
    ///
    /// # Panics
    ///
    /// Panics when the lease is not counted, when cleanup does not wait for it,
    /// or when dropping the reader does not release it.
    #[tokio::test(flavor = "current_thread")]
    async fn cleanup_waits_for_a_pinned_reader_to_release_its_staged_lease() {
        let registry = std::sync::Arc::new(ScribeHotSourceRegistry::new());
        let key = seal_key();
        let generation = GenerationOrdinal::new(0, 1);
        // The staged member is a real file on disk, because the readability the
        // lease protects is the reader's ability to open that run after cleanup
        // has already decided to delete it.
        let stage = tempfile::tempdir().expect("a staging directory");
        let run = stage.path().join("run-1.parquet");
        std::fs::write(&run, b"staged run bytes").expect("the staged run is written");
        registry
            .register_memtable(&key, generation)
            .expect("generation registers");
        registry
            .advance(&key, generation, staged_at(run.clone()))
            .expect("generation stages");

        let lease = registry
            .staged_sources(key.tenant, &key.table, key.partition, key.partition)
            .expect("locked");
        assert_eq!(lease.sources().len(), 1);
        assert_eq!(registry.leases(&key, generation).expect("locked"), 1);
        let second = registry
            .staged_sources(key.tenant, &key.table, key.partition, key.partition)
            .expect("locked");
        assert_eq!(second.sources().len(), 1);
        assert_eq!(
            registry.leases(&key, generation).expect("locked"),
            2,
            "each pinned reader owns its own staged lease"
        );

        let waiting = registry.drain_leases(&key, generation);
        tokio::pin!(waiting);
        assert!(
            futures_util::poll!(waiting.as_mut()).is_pending(),
            "cleanup must not proceed while a reader holds the member open"
        );

        // Cleanup is now blocked on this lease, which is exactly the moment a
        // pinned reader opens the run it resolved. The lease is only worth
        // anything if those bytes are still there.
        let named = lease.sources()[0].runs[0].clone();
        assert_eq!(named, run);
        assert_eq!(
            std::fs::read(&named).expect("a pinned staged run is still readable"),
            b"staged run bytes",
            "the staged source a lease names stays readable while cleanup waits"
        );

        drop(lease);
        assert_eq!(
            registry.leases(&key, generation).expect("locked"),
            1,
            "one reader's drop releases exactly its own lease"
        );
        assert!(
            futures_util::poll!(waiting.as_mut()).is_pending(),
            "cleanup waits for the last pinned reader, not the first"
        );
        assert!(
            std::fs::read(&second.sources()[0].runs[0]).is_ok(),
            "the surviving reader's staged run is still readable after the first drop"
        );

        drop(second);
        assert_eq!(registry.leases(&key, generation).expect("locked"), 0);
        waiting
            .await
            .expect("cleanup proceeds once the reader is gone");
        assert_eq!(
            registry.leases(&key, generation).expect("locked"),
            0,
            "a settled staged lease is never released a second time"
        );
        registry
            .drain_leases(&key, generation)
            .await
            .expect("a settled generation drains again without waiting");
    }

    /// Draining a member no reader holds returns without waiting.
    ///
    /// # Panics
    ///
    /// Panics when an unleased member makes cleanup wait, which would stall
    /// every publication that has no concurrent read at all.
    #[tokio::test(flavor = "current_thread")]
    async fn cleanup_does_not_wait_on_a_member_no_reader_holds() {
        let registry = std::sync::Arc::new(ScribeHotSourceRegistry::new());
        let key = seal_key();
        let generation = GenerationOrdinal::new(0, 1);
        registry
            .register_memtable(&key, generation)
            .expect("generation registers");
        registry
            .advance(&key, generation, staged())
            .expect("generation stages");

        registry
            .drain_leases(&key, generation)
            .await
            .expect("an unleased member drains immediately");
    }
}
