//! Deterministic, IO-free Forge planning and capacity classification.

use sha2::{Digest, Sha256};
use vala_sql::row_types::forge_tasks::{
    FORGE_TASK_PAYLOAD_VERSION, ForgeTaskEstimates, ForgeTaskLane, ForgeTaskPlan, ForgeTaskStrategy,
};

use super::ForgeConfig;
use super::error::ForgeError;

/// Every hard ceiling used to classify exact Forge plans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForgeCapacity {
    /// Maximum ordinary input files.
    pub max_files: u32,
    /// Maximum ordinary input bytes.
    pub max_bytes: u64,
    /// Maximum planned parallelism.
    pub max_parallelism: u16,
    /// Maximum peak memory estimate.
    pub max_memory_bytes: u64,
    /// Maximum spill estimate.
    pub max_spill_bytes: u64,
    /// Maximum bytes for one cluster-fenced singleton.
    pub max_large_task_bytes: u64,
}

impl ForgeCapacity {
    /// Validates that every safety ceiling is positive and the large lane covers bytes.
    ///
    /// # Errors
    /// Returns [`ForgeError::InvalidConfig`] for a zero ceiling or inverted byte lanes.
    pub fn validate(self) -> Result<Self, ForgeError> {
        if self.max_files == 0
            || self.max_bytes == 0
            || self.max_parallelism == 0
            || self.max_memory_bytes == 0
            || self.max_spill_bytes == 0
            || self.max_large_task_bytes < self.max_bytes
        {
            return Err(ForgeError::InvalidConfig {
                detail: "every Forge capacity must be positive and the large lane must cover the ordinary byte lane".to_owned(),
            });
        }
        Ok(self)
    }
}

impl TryFrom<&ForgeConfig> for ForgeCapacity {
    /// Configuration validation failure returned when capacity cannot be represented.
    type Error = ForgeError;

    /// Converts every configured planning and admission ceiling into its durable domain.
    ///
    /// The conversion is the single capacity construction path shared by the
    /// scheduler and worker, so neither consumer can omit or reinterpret a field.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] when a count exceeds its durable
    /// integer domain or any capacity relationship is invalid.
    fn try_from(config: &ForgeConfig) -> Result<Self, Self::Error> {
        Self {
            max_files: u32::try_from(config.max_files_per_tick).map_err(|_| {
                ForgeError::InvalidConfig {
                    detail: "Forge file capacity exceeds u32".to_owned(),
                }
            })?,
            max_bytes: config.max_bytes_per_tick,
            max_parallelism: u16::try_from(config.max_concurrent_reads).map_err(|_| {
                ForgeError::InvalidConfig {
                    detail: "Forge parallelism exceeds u16".to_owned(),
                }
            })?,
            max_memory_bytes: config.max_memory_bytes,
            max_spill_bytes: config.spill_limit_bytes,
            max_large_task_bytes: config.max_large_task_bytes,
        }
        .validate()
    }
}

/// One exact candidate derived from a single stable table snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgePlanCandidate {
    /// Closed maintenance strategy.
    pub strategy: ForgeTaskStrategy,
    /// Sorted, duplicate-free immutable input identities.
    pub inputs: Vec<String>,
    /// Estimated total input bytes.
    pub bytes: u64,
    /// Planned bounded parallelism.
    pub parallelism: u16,
    /// Estimated peak memory.
    pub memory_bytes: u64,
    /// Estimated spill use.
    pub spill_bytes: u64,
    /// Stable strategy parameters.
    pub parameters: serde_json::Value,
}

/// Complete stable metadata view supplied after catalog IO finishes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeTableSnapshot {
    /// Exact Iceberg snapshot identity shared by every emitted plan.
    pub snapshot_id: i64,
    /// Deterministically ordered candidates from that snapshot.
    pub candidates: Vec<ForgePlanCandidate>,
}

/// Closed capacity result assigned exactly once to every candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgePlanCapacity {
    /// The plan fits every ordinary ceiling.
    Ordinary,
    /// One task fits every non-byte ceiling and requires the cluster-fenced large lane.
    LargeSingleton,
    /// The plan exceeds at least one non-relaxable ceiling.
    Unschedulable,
}

/// Exact versioned plan plus its sole capacity classification.
#[derive(Debug, Clone)]
pub struct PlannedForgeTask {
    /// Closed strategy persisted with the task.
    pub strategy: ForgeTaskStrategy,
    /// Stable snapshot used for discovery and planning.
    pub base_snapshot_id: i64,
    /// Exact versioned payload.
    pub plan: ForgeTaskPlan,
    /// SHA-256 of the canonical payload bytes.
    pub plan_hash: [u8; 32],
    /// Positive persisted admission estimates.
    pub estimates: ForgeTaskEstimates,
    /// Sole admission classification.
    pub capacity: ForgePlanCapacity,
}

impl PlannedForgeTask {
    /// Returns the durable lane for executable work.
    ///
    /// # Errors
    /// Returns an invariant error when called for terminally unschedulable work.
    pub fn lane(&self) -> Result<ForgeTaskLane, ForgeError> {
        match self.capacity {
            ForgePlanCapacity::Ordinary => Ok(ForgeTaskLane::Ordinary),
            ForgePlanCapacity::LargeSingleton => Ok(ForgeTaskLane::LargeSingleton),
            ForgePlanCapacity::Unschedulable => Err(ForgeError::Invariant {
                detail: "unschedulable Forge plan has no executable lane".to_owned(),
            }),
        }
    }
}

/// Stateful owner of deterministic plan construction.
#[derive(Debug, Clone, Copy)]
pub struct ForgePlanner {
    /// Validated ceilings used for every plan produced by this owner.
    capacity: ForgeCapacity,
}

impl ForgePlanner {
    /// Constructs the one process-independent planner owner from validated capacity.
    ///
    /// Callers construct `capacity` through [`ForgeCapacity::try_from`] so all
    /// planning operations share the same checked ceiling relationships.
    #[must_use]
    pub const fn new(capacity: ForgeCapacity) -> Self {
        Self { capacity }
    }

    /// Plans every candidate from one stable snapshot and assigns one capacity outcome.
    ///
    /// # Errors
    /// Returns an invariant error for empty, unsorted, duplicate, or zero-valued candidates.
    pub fn plan_table(
        &self,
        snapshot: &ForgeTableSnapshot,
    ) -> Result<Vec<PlannedForgeTask>, ForgeError> {
        snapshot
            .candidates
            .iter()
            .map(|candidate| self.plan_candidate(snapshot.snapshot_id, candidate))
            .collect()
    }

    /// Builds and hashes one exact candidate without performing IO.
    ///
    /// # Errors
    ///
    /// Returns an invariant error when candidate identity, estimates, or
    /// canonical payload encoding cannot produce a durable plan.
    fn plan_candidate(
        &self,
        snapshot_id: i64,
        candidate: &ForgePlanCandidate,
    ) -> Result<PlannedForgeTask, ForgeError> {
        let files = u32::try_from(candidate.inputs.len()).map_err(|_| ForgeError::Invariant {
            detail: "Forge plan input count exceeds u32".to_owned(),
        })?;
        if files == 0
            || candidate.bytes == 0
            || candidate.parallelism == 0
            || candidate.memory_bytes == 0
            || candidate.spill_bytes == 0
            || candidate.inputs.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(ForgeError::Invariant {
                detail: "Forge candidate inputs and estimates must be positive, sorted, and unique"
                    .to_owned(),
            });
        }
        let ordinary = files <= self.capacity.max_files
            && candidate.bytes <= self.capacity.max_bytes
            && candidate.parallelism <= self.capacity.max_parallelism
            && candidate.memory_bytes <= self.capacity.max_memory_bytes
            && candidate.spill_bytes <= self.capacity.max_spill_bytes;
        let capacity_outcome = if ordinary {
            ForgePlanCapacity::Ordinary
        } else if files == 1
            && candidate.bytes <= self.capacity.max_large_task_bytes
            && candidate.parallelism <= self.capacity.max_parallelism
            && candidate.memory_bytes <= self.capacity.max_memory_bytes
            && candidate.spill_bytes <= self.capacity.max_spill_bytes
        {
            ForgePlanCapacity::LargeSingleton
        } else {
            ForgePlanCapacity::Unschedulable
        };
        let plan = ForgeTaskPlan {
            version: FORGE_TASK_PAYLOAD_VERSION,
            inputs: candidate.inputs.clone(),
            parameters: candidate.parameters.clone(),
        };
        let canonical = serde_json::to_vec(&serde_json::json!({
            "inputs": plan.inputs,
            "parameters": plan.parameters,
            "version": plan.version,
        }))
        .map_err(|error| ForgeError::Invariant {
            detail: error.to_string(),
        })?;
        let plan_hash: [u8; 32] = Sha256::digest(canonical).into();
        Ok(PlannedForgeTask {
            strategy: candidate.strategy,
            base_snapshot_id: snapshot_id,
            plan,
            plan_hash,
            estimates: ForgeTaskEstimates {
                files,
                bytes: candidate.bytes,
                parallelism: candidate.parallelism,
                memory_bytes: candidate.memory_bytes,
                spill_bytes: candidate.spill_bytes,
                large_ceiling_bytes: self.capacity.max_large_task_bytes,
            },
            capacity: capacity_outcome,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns capacity with independently testable ceilings.
    fn capacity() -> ForgeCapacity {
        ForgeCapacity {
            max_files: 4,
            max_bytes: 100,
            max_parallelism: 2,
            max_memory_bytes: 80,
            max_spill_bytes: 60,
            max_large_task_bytes: 200,
        }
    }

    /// Returns one valid candidate whose selected estimate can be changed.
    fn candidate() -> ForgePlanCandidate {
        ForgePlanCandidate {
            strategy: ForgeTaskStrategy::SmallFiles,
            inputs: vec!["a.parquet".to_owned()],
            bytes: 50,
            parallelism: 1,
            memory_bytes: 40,
            spill_bytes: 30,
            parameters: serde_json::json!({"target":64}),
        }
    }

    /// Every capacity dimension independently controls the sole outcome.
    #[test]
    fn planner_classifies_all_capacity_dimensions_and_singleton_lane() {
        let owner = ForgePlanner::new(capacity());
        for mutate in [
            |c: &mut ForgePlanCandidate| {
                c.inputs = (0..5).map(|i| format!("{i}.parquet")).collect();
            },
            |c: &mut ForgePlanCandidate| c.bytes = 201,
            |c: &mut ForgePlanCandidate| c.parallelism = 3,
            |c: &mut ForgePlanCandidate| c.memory_bytes = 81,
            |c: &mut ForgePlanCandidate| c.spill_bytes = 61,
        ] {
            let mut value = candidate();
            mutate(&mut value);
            let tasks = owner
                .plan_table(&ForgeTableSnapshot {
                    snapshot_id: 7,
                    candidates: vec![value],
                })
                .expect("valid plan");
            assert_eq!(tasks[0].capacity, ForgePlanCapacity::Unschedulable);
        }
        let mut large = candidate();
        large.bytes = 150;
        let tasks = owner
            .plan_table(&ForgeTableSnapshot {
                snapshot_id: 7,
                candidates: vec![large],
            })
            .expect("large plan");
        assert_eq!(tasks[0].capacity, ForgePlanCapacity::LargeSingleton);

        let mut multi_file_overflow = candidate();
        multi_file_overflow.inputs = vec!["a.parquet".to_owned(), "b.parquet".to_owned()];
        multi_file_overflow.bytes = 150;
        let tasks = owner
            .plan_table(&ForgeTableSnapshot {
                snapshot_id: 7,
                candidates: vec![multi_file_overflow],
            })
            .expect("multi-file overflow plan");
        assert_eq!(tasks[0].capacity, ForgePlanCapacity::Unschedulable);
    }

    /// Configuration conversion copies every field and rejects invalid relationships.
    #[test]
    fn capacity_conversion_is_field_complete_and_validated() {
        let mut config = ForgeConfig::default();
        config.max_files_per_tick = 17;
        config.max_bytes_per_tick = 101;
        config.max_concurrent_reads = 3;
        config.max_memory_bytes = 103;
        config.spill_limit_bytes = 107;
        config.max_large_task_bytes = 109;
        assert_eq!(
            ForgeCapacity::try_from(&config).expect("valid capacity"),
            ForgeCapacity {
                max_files: 17,
                max_bytes: 101,
                max_parallelism: 3,
                max_memory_bytes: 103,
                max_spill_bytes: 107,
                max_large_task_bytes: 109,
            }
        );

        config.max_large_task_bytes = 100;
        assert!(matches!(
            ForgeCapacity::try_from(&config),
            Err(ForgeError::InvalidConfig { .. })
        ));
    }

    /// Stable snapshots yield stable hashes while replanning a new snapshot changes identity.
    #[test]
    fn canonical_plan_hash_is_stable_and_snapshot_is_exact() {
        let planner = ForgePlanner::new(capacity());
        let first = planner
            .plan_table(&ForgeTableSnapshot {
                snapshot_id: 7,
                candidates: vec![candidate()],
            })
            .expect("first");
        let replay = planner
            .plan_table(&ForgeTableSnapshot {
                snapshot_id: 7,
                candidates: vec![candidate()],
            })
            .expect("replay");
        assert_eq!(first[0].plan_hash, replay[0].plan_hash);
        assert_eq!(first[0].base_snapshot_id, 7);
        let replanned = planner
            .plan_table(&ForgeTableSnapshot {
                snapshot_id: 8,
                candidates: vec![candidate()],
            })
            .expect("snapshot replan");
        assert_ne!(first[0].base_snapshot_id, replanned[0].base_snapshot_id);
        assert_eq!(
            first[0].plan_hash, replanned[0].plan_hash,
            "plan hash is canonical payload identity while snapshot remains a separate idempotency dimension"
        );
    }
}
