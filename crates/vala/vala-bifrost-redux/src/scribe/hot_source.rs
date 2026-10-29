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
use std::sync::{Arc, Mutex};

use parquet::arrow::arrow_reader::ArrowReaderMetadata;

use crate::scan_sizing::{ScanScope, ScanSizer};
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

/// The footers of one staged member's runs, each with the run's exact encoded
/// size, in run order.
///
/// Read once, by [`crate::scribe::member_stager::preflight_run`] when the
/// runs are staged or recovered, so the Scribe listing sizes a scan of them
/// through [`ScanSizer`] and a live read hands each run to Oracle's Parquet
/// scan with its size, rows and bounds, without touching the disk.
#[derive(Debug, Clone, Default)]
pub struct RunFooters(Arc<[(u64, ArrowReaderMetadata)]>);

impl RunFooters {
    /// Wraps each run's exact encoded size and footer, in run order.
    #[must_use]
    pub fn new(runs: Vec<(u64, ArrowReaderMetadata)>) -> Self {
        Self(runs.into())
    }

    /// Returns the bytes `scope` reads from the runs, each sized by
    /// [`ScanSizer::footer_size`] under the first run's Arrow schema, which
    /// every run of one member shares; no runs read no bytes.
    #[must_use]
    pub fn scoped_bytes(&self, scope: &ScanScope) -> u64 {
        let Some((_, first)) = self.0.first() else {
            return 0;
        };
        let sizer = ScanSizer::new(scope, first.schema());
        self.0
            .iter()
            .map(|(_, run)| sizer.footer_size(run.metadata()).bytes)
            .fold(0_u64, u64::saturating_add)
    }

    /// Reads the size and footer of each run in `paths`, as staging records
    /// them.
    ///
    /// # Panics
    ///
    /// Panics when a run cannot be read or preflighted.
    #[cfg(test)]
    pub(crate) fn read_for_test(paths: &[PathBuf]) -> Self {
        Self::new(
            paths
                .iter()
                .map(|path| {
                    (
                        std::fs::metadata(path).expect("staged run size").len(),
                        crate::scribe::member_stager::preflight_run(path)
                            .expect("staged run footer"),
                    )
                })
                .collect(),
        )
    }

    /// Pairs each of `paths`, the member's runs in the same run order, with
    /// its recorded size and footer.
    pub(crate) fn reads<'a>(
        &'a self,
        paths: &'a [PathBuf],
    ) -> impl Iterator<Item = StagedRunRead> + 'a {
        paths
            .iter()
            .zip(self.0.iter())
            .map(|(path, (bytes, footer))| StagedRunRead {
                path: path.clone(),
                bytes: *bytes,
                footer: Arc::clone(footer.metadata()),
            })
    }
}

/// One staged run a live read opens: its path and the exact facts staging
/// recorded for it.
#[derive(Debug, Clone)]
pub struct StagedRunRead {
    /// Fsynced local path of the run.
    pub path: PathBuf,
    /// Exact encoded bytes of the run.
    pub bytes: u64,
    /// The run's footer, which carries its row count and row-group bounds.
    pub footer: Arc<parquet::file::metadata::ParquetMetaData>,
}

/// Footers are derived from the runs, which already decide identity, so they
/// never make two authorities differ.
impl PartialEq for RunFooters {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for RunFooters {}

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
        /// The runs' footers, which size a scan of them.
        footers: RunFooters,
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
    /// Validated run footers, which size the member for one scan scope.
    pub footers: RunFooters,
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
    /// Generations whose publication may have committed in the catalog while
    /// their authority here still names the staged runs.
    ///
    /// Marked before the fenced commit is attempted and cleared only once the
    /// outcome is settled here: by the advance to `Published`, by release, or
    /// by a commit known not to have landed. A capture refuses to read a table
    /// while any of its generations is marked, because both the published
    /// file and the staged runs may then serve the same rows.
    publishing: std::collections::HashSet<GenerationOrdinal>,
}

impl KeyAuthorities {
    /// Clears the publication marks of `generations`, reporting whether any
    /// mark was removed.
    fn settle(&mut self, generations: impl IntoIterator<Item = GenerationOrdinal>) -> bool {
        let mut settled = false;
        for generation in generations {
            settled |= self.publishing.remove(&generation);
        }
        settled
    }

    /// Reports whether this key holds no generation and no publication mark.
    fn is_empty(&self) -> bool {
        self.by_generation.is_empty() && self.publishing.is_empty()
    }
}

/// One coherent staged half of a live capture.
///
/// Taken in the same registry critical section that decides which memtable
/// generations the capture may still read, so a generation is served by
/// exactly one of its memory copy, its staged runs, or a published object.
#[derive(Debug)]
pub struct StagedCapture {
    /// Lease over the staged members the memory half did not already serve.
    pub lease: StagedSourceLease,
    /// Memory-served generations whose authority is already a published
    /// object; the capture must drop their memory copy.
    pub superseded: std::collections::HashSet<(SealKey, GenerationOrdinal)>,
    /// The table revision this node's last locally completed publication
    /// committed at, read in the same critical section: every generation
    /// the capture omits as published is covered by a cut at least this new.
    pub publication_revision: i64,
}

/// Highest committed publication revision this node has advanced per tenant
/// table.
type PublicationRevisions = HashMap<(wyrd_spec::ids::DataTenantId, crate::catalog::TableRef), i64>;

/// Pod-wide registry of where every live generation's rows are readable.
#[derive(Debug, Default)]
pub struct ScribeHotSourceRegistry {
    /// Authorities per seal key.
    state: Mutex<HashMap<SealKey, KeyAuthorities>>,
    /// Signalled whenever a lease is released, so cleanup can wait on it.
    released: tokio::sync::Notify,
    /// Signalled whenever a publication mark is cleared, so a capture
    /// refused by one can retry.
    settled: tokio::sync::Notify,
    /// Each table's revision as of this node's last locally completed
    /// publication, never decreasing.
    ///
    /// Locked only while `state` is held, so a capture reads the revision
    /// in the same critical section that decides which generations it
    /// still serves, and an advance to `Published` raises it in the same
    /// critical section that stops serving them.
    revisions: Mutex<PublicationRevisions>,
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
        let authorities = state.get_mut(key).ok_or(HotSourceError::Unregistered {
            generation: generation.get(),
        })?;
        let current =
            authorities
                .by_generation
                .get_mut(&generation)
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
        let published = matches!(next, HotAuthority::Published { .. });
        *current = next;
        if published && authorities.settle([generation]) {
            self.settled.notify_waiters();
        }
        Ok(())
    }

    /// Atomically advances every named generation under one seal key, the
    /// moves of one committed publication at `publication_revision`.
    ///
    /// The registry prevalidates the complete set while holding its one state
    /// lock, then applies every move and raises the table's revision to
    /// `publication_revision` in the same critical section. An exact replay is
    /// idempotent; a missing generation, backwards move, or same-stage
    /// identity contradiction leaves the complete set and the revision
    /// unchanged.
    ///
    /// # Errors
    ///
    /// Returns a typed hot-source error when any member cannot make the exact
    /// requested transition, or when the registry lock is poisoned.
    pub fn advance_all_atomic(
        &self,
        key: &SealKey,
        transitions: &[(GenerationOrdinal, HotAuthority)],
        publication_revision: i64,
    ) -> Result<(), HotSourceError> {
        let mut state = self.lock()?;
        let mut revisions = self.lock_revisions()?;
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
        let revision = revisions
            .entry((key.tenant, key.table.clone()))
            .or_default();
        *revision = (*revision).max(publication_revision);
        drop(revisions);
        let published = transitions
            .iter()
            .filter(|(_, next)| matches!(next, HotAuthority::Published { .. }))
            .map(|(generation, _)| *generation);
        if authorities.settle(published) {
            self.settled.notify_waiters();
        }
        Ok(())
    }

    /// Marks generations whose fenced publication is about to be attempted.
    ///
    /// From this call until the outcome is settled here, the catalog may
    /// already serve these generations' rows while their authority still
    /// names the staged runs, so [`Self::capture_staged`] refuses their table.
    /// The mark is cleared by the advance to `Published`, by release, or by
    /// [`Self::abandon_publication`]; an unresolved outcome leaves it set.
    ///
    /// # Errors
    ///
    /// Returns [`HotSourceError::Unregistered`] when a generation is unknown,
    /// which leaves no mark, and [`HotSourceError::Poisoned`] on a poisoned
    /// lock.
    pub fn begin_publication(
        &self,
        key: &SealKey,
        generations: &[GenerationOrdinal],
    ) -> Result<(), HotSourceError> {
        let mut state = self.lock()?;
        let unregistered = |generation: GenerationOrdinal| HotSourceError::Unregistered {
            generation: generation.get(),
        };
        let authorities = state.get_mut(key).ok_or_else(|| {
            generations.first().map_or(
                HotSourceError::Unregistered { generation: 0 },
                |generation| unregistered(*generation),
            )
        })?;
        if let Some(missing) = generations
            .iter()
            .find(|generation| !authorities.by_generation.contains_key(generation))
        {
            return Err(unregistered(*missing));
        }
        authorities.publishing.extend(generations.iter().copied());
        Ok(())
    }

    /// Clears publication marks for a commit known not to have landed.
    ///
    /// The staged runs remain the generations' only durable authority, so a
    /// capture may read them again. Clearing an unmarked generation is a
    /// no-op.
    ///
    /// # Errors
    ///
    /// Returns [`HotSourceError::Poisoned`] on a poisoned lock.
    pub fn abandon_publication(
        &self,
        key: &SealKey,
        generations: &[GenerationOrdinal],
    ) -> Result<(), HotSourceError> {
        let mut state = self.lock()?;
        let Some(authorities) = state.get_mut(key) else {
            return Ok(());
        };
        let settled = authorities.settle(generations.iter().copied());
        if authorities.is_empty() {
            state.remove(key);
        }
        drop(state);
        if settled {
            self.settled.notify_waiters();
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
        let settled = authorities.settle([generation]);
        if authorities.is_empty() {
            state.remove(key);
        }
        drop(state);
        if settled {
            self.settled.notify_waiters();
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
        let settled = authorities.settle([generation]);
        if authorities.is_empty() {
            state.remove(key);
        }
        drop(state);
        if settled {
            self.settled.notify_waiters();
        }
        Ok(())
    }

    /// Raises each table's revision to one an earlier process of this node
    /// committed, before this process serves any capture.
    ///
    /// A restarted Scribe no longer holds the generations it published
    /// before, so without this it would report a lower revision than a cut
    /// that lacks those rows needs to be caught by.
    ///
    /// # Errors
    ///
    /// Returns [`HotSourceError::Poisoned`] on a poisoned lock.
    pub fn restore_publication_revisions(
        &self,
        restored: impl IntoIterator<
            Item = (
                (wyrd_spec::ids::DataTenantId, crate::catalog::TableRef),
                i64,
            ),
        >,
    ) -> Result<(), HotSourceError> {
        let _state = self.lock()?;
        let mut revisions = self.lock_revisions()?;
        for (table, publication_revision) in restored {
            let revision = revisions.entry(table).or_default();
            *revision = (*revision).max(publication_revision);
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

    /// Takes the staged half of one coherent whole-table live capture.
    ///
    /// The caller holds the memory half's memtable read guards and passes the
    /// immutable generations that half serves, keyed by their complete seal
    /// key, since generation ordinals restart per key and two revisions of one
    /// partition can share one. In one
    /// registry critical section this then decides every generation's single
    /// source: a served generation whose authority is already a published
    /// object is reported as superseded so its memory copy is dropped; any
    /// other served generation stays in memory and its staged runs are skipped;
    /// and every unserved staged generation is leased. Sources are ordered by
    /// partition and then generation so a reader scans rows in age order.
    ///
    /// Returns `None`, leasing nothing, while any generation of the table is
    /// marked by [`Self::begin_publication`]: its rows may then be served by
    /// both a published file and its staged runs, and no choice between them
    /// is provably right. The caller waits for [`Self::publication_settled`]
    /// and captures again.
    ///
    /// # Errors
    ///
    /// Returns [`HotSourceError::Poisoned`] on a poisoned lock.
    pub fn capture_staged(
        self: &Arc<Self>,
        tenant: wyrd_spec::ids::DataTenantId,
        table: &crate::catalog::TableRef,
        served: &std::collections::HashSet<(SealKey, GenerationOrdinal)>,
    ) -> Result<Option<StagedCapture>, HotSourceError> {
        let mut state = self.lock()?;
        let publication_revision = self
            .lock_revisions()?
            .get(&(tenant, table.clone()))
            .copied()
            .unwrap_or(0);
        let selected = |key: &SealKey| key.tenant == tenant && key.table == *table;
        if state
            .iter()
            .any(|(key, authorities)| selected(key) && !authorities.publishing.is_empty())
        {
            return Ok(None);
        }
        let mut sources = Vec::new();
        let mut superseded = std::collections::HashSet::new();
        for (key, authorities) in state.iter() {
            if !selected(key) {
                continue;
            }
            for (generation, authority) in &authorities.by_generation {
                if served.contains(&(key.clone(), *generation)) {
                    if matches!(authority, HotAuthority::Published { .. }) {
                        superseded.insert((key.clone(), *generation));
                    }
                    continue;
                }
                let HotAuthority::StagedRun {
                    member,
                    runs,
                    bytes,
                    footers,
                    wal,
                } = authority
                else {
                    continue;
                };
                sources.push(StagedSource {
                    key: key.clone(),
                    generation: *generation,
                    member: *member,
                    runs: runs.clone(),
                    bytes: *bytes,
                    footers: footers.clone(),
                    wal: *wal,
                });
            }
        }
        let held: Vec<_> = sources
            .iter()
            .map(|source| (source.key.clone(), source.generation))
            .collect();
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
        Ok(Some(StagedCapture {
            lease: StagedSourceLease {
                registry: Arc::clone(self),
                held,
                sources,
            },
            superseded,
            publication_revision,
        }))
    }

    /// Returns a future that completes once a publication mark is cleared.
    ///
    /// Create it before the capture attempt it follows: a mark cleared after
    /// creation and before the first poll still completes it.
    pub fn publication_settled(&self) -> tokio::sync::futures::Notified<'_> {
        self.settled.notified()
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

    /// Locks the publication revisions; callers already hold `state`.
    ///
    /// # Errors
    ///
    /// Returns [`HotSourceError::Poisoned`] when a previous holder panicked.
    fn lock_revisions(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, PublicationRevisions>, HotSourceError> {
        self.revisions
            .lock()
            .map_err(|error| HotSourceError::Poisoned {
                detail: error.to_string(),
            })
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
    use crate::catalog::{PhysicalPartition, TableRef, TimeGranularity, TimePartition};
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
            footers: RunFooters::default(),
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
            registry.advance_all_atomic(&key, &transitions, 1),
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
            .advance_all_atomic(&key, &transitions, 1)
            .expect("the complete retry publishes atomically");
        registry
            .advance_all_atomic(&key, &transitions, 1)
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

    /// A staged capture sees exactly the generations the staged boundary owns,
    /// in age order, scoped to the requested tenant table.
    ///
    /// # Panics
    ///
    /// Panics when a fixture registration or transition is refused.
    #[test]
    fn staged_captures_are_scoped_ordered_and_exclude_other_authorities() {
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
            key.partition.clone(),
        );
        let other_tenant = SealKey::new(
            DataTenantId::new_v7(),
            key.table.clone(),
            key.partition.clone(),
        );
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

        let capture = registry
            .capture_staged(key.tenant, &key.table, &std::collections::HashSet::new())
            .expect("locked")
            .expect("no publication is marked");
        let named: Vec<(PhysicalPartition, u64)> = capture
            .lease
            .sources()
            .iter()
            .map(|source| (source.key.partition.clone(), source.generation.get()))
            .collect();
        assert_eq!(
            named,
            vec![
                (key.partition.clone(), 1),
                (key.partition.clone(), 2),
                (next_day.partition.clone(), 3),
            ],
            "only staged generations of the requested table are readable, oldest first"
        );
    }

    /// A generation whose publication may have committed refuses capture of
    /// its table until the outcome settles, and a served generation that is
    /// already published is reported as superseded.
    ///
    /// # Panics
    ///
    /// Panics when a fixture transition is refused or a capture is taken or
    /// refused contrary to the publication mark.
    #[tokio::test]
    async fn a_publishing_generation_refuses_capture_until_it_settles() {
        let registry = std::sync::Arc::new(ScribeHotSourceRegistry::new());
        let key = seal_key();
        let generation = GenerationOrdinal::new(0, 1);
        registry
            .register_memtable(&key, generation)
            .expect("generation registers");
        registry
            .advance(&key, generation, staged())
            .expect("generation stages");
        let none = std::collections::HashSet::new();
        registry
            .begin_publication(&key, &[generation])
            .expect("publication is marked");
        let settled = registry.publication_settled();
        assert!(
            registry
                .capture_staged(key.tenant, &key.table, &none)
                .expect("locked")
                .is_none(),
            "a possibly committed publication refuses capture"
        );
        registry
            .abandon_publication(&key, &[generation])
            .expect("the commit did not land");
        settled.await;
        assert_eq!(
            registry
                .capture_staged(key.tenant, &key.table, &none)
                .expect("locked")
                .expect("an abandoned publication no longer refuses")
                .lease
                .sources()
                .len(),
            1
        );

        registry
            .begin_publication(&key, &[generation])
            .expect("publication is marked again");
        registry
            .advance(
                &key,
                generation,
                HotAuthority::Published {
                    object_keys: vec!["objects/one.parquet".to_owned()],
                },
            )
            .expect("generation publishes");
        let served = std::collections::HashSet::from([(key.clone(), generation)]);
        let capture = registry
            .capture_staged(key.tenant, &key.table, &served)
            .expect("locked")
            .expect("publication settled by the advance");
        assert!(capture.lease.sources().is_empty());
        assert_eq!(capture.superseded, served, "the memory copy is superseded");
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
            .capture_staged(key.tenant, &key.table, &std::collections::HashSet::new())
            .expect("locked")
            .expect("no publication is marked")
            .lease;
        assert_eq!(lease.sources().len(), 1);
        assert_eq!(registry.leases(&key, generation).expect("locked"), 1);
        let second = registry
            .capture_staged(key.tenant, &key.table, &std::collections::HashSet::new())
            .expect("locked")
            .expect("no publication is marked")
            .lease;
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
