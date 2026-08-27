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

use crate::scribe::seal_key::SealKey;

/// Stable identity of one Scribe generation within its seal key.
///
/// Monotonic per key. Comparison is meaningful: a lower ordinal is always an
/// older generation of the same key, which is what makes "authority only moves
/// forward" checkable rather than merely intended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GenerationOrdinal(u64);

impl GenerationOrdinal {
    /// Builds an ordinal from its raw monotonic value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the raw monotonic value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
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
    /// Readable from a durable local sorted run fsynced under the staging root.
    ///
    /// Reaching this state is the first durable boundary: the rows survive a
    /// process restart without the WAL, which is exactly what permits the WAL
    /// segments behind them to be retired.
    StagedRun {
        /// Fsynced local run path serving the rows.
        path: PathBuf,
        /// Encoded bytes the run occupies on the staging volume.
        bytes: u64,
    },
    /// Readable from a published hot object recorded in the catalog.
    Published {
        /// Remote object key the catalog row points at.
        object_key: String,
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

/// Live authority for one seal key's generations.
#[derive(Debug, Default)]
struct KeyAuthorities {
    /// Current authority per registered generation.
    by_generation: HashMap<GenerationOrdinal, HotAuthority>,
}

/// Pod-wide registry of where every live generation's rows are readable.
#[derive(Debug, Default)]
pub struct ScribeHotSourceRegistry {
    /// Authorities per seal key.
    state: Mutex<HashMap<SealKey, KeyAuthorities>>,
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
        let current = authorities
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
        Ok(self.lock()?.get(key).and_then(|authorities| {
            authorities.by_generation.get(&generation).cloned()
        }))
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
        HotAuthority::StagedRun {
            path: PathBuf::from("/stage/run-1.parquet"),
            bytes: 1_024,
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
        let generation = GenerationOrdinal::new(1);
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
            object_key: "hot/events/0001.parquet".to_owned(),
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
        let generation = GenerationOrdinal::new(7);
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
        let released = registry.release(&key, generation).expect("release succeeds");
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
                .register_memtable(&key, GenerationOrdinal::new(ordinal))
                .expect("registers");
        }
        registry
            .advance(&key, GenerationOrdinal::new(1), staged())
            .expect("stages the oldest");
        registry
            .advance(&key, GenerationOrdinal::new(3), staged())
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
            .advance(&key, GenerationOrdinal::new(2), staged())
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
        let generation = GenerationOrdinal::new(9);
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
}
