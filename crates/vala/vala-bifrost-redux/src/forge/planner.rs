//! Deterministic, IO-free Forge planning and capacity classification.

use sha2::{Digest, Sha256};
use vala_sql::row_types::forge_tasks::{
    FORGE_ENVELOPE_VERSION, FORGE_TASK_PAYLOAD_VERSION, ForgeTaskEnvelope, ForgeTaskEstimates,
    ForgeTaskLane, ForgeTaskPlan, ForgeTaskStrategy,
};

use super::ForgeConfig;
use super::error::ForgeError;

/// Every hard ceiling used to classify exact Forge plans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForgeCapacity {
    /// Maximum planned parallelism.
    pub max_parallelism: u16,
    /// Maximum peak memory estimate.
    pub max_memory_bytes: u64,
    /// Maximum spill estimate.
    pub max_spill_bytes: u64,
    /// Maximum bytes for one large-lane singleton admitted under a worker's own
    /// per-worker capacity budget.
    pub max_large_task_bytes: u64,
}

impl ForgeCapacity {
    /// Validates that every safety ceiling is positive.
    ///
    /// # Errors
    /// Returns [`ForgeError::InvalidConfig`] for a zero ceiling.
    pub fn validate(self) -> Result<Self, ForgeError> {
        if self.max_parallelism == 0
            || self.max_memory_bytes == 0
            || self.max_spill_bytes == 0
            || self.max_large_task_bytes == 0
        {
            return Err(ForgeError::InvalidConfig {
                detail: "every Forge capacity ceiling must be positive".to_owned(),
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
    /// Per-input bytes aligned with `inputs` for deterministic capacity splitting.
    pub input_bytes: Vec<u64>,
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

/// Every deterministic candidate derived from one immutable table snapshot.
///
/// The snapshot identity travels with the candidates so a plan can never be
/// persisted against a base the discovery pass did not actually observe.
#[derive(Debug, Clone)]
pub struct ForgeTableSnapshot {
    /// Exact Iceberg snapshot identity shared by every emitted plan.
    pub snapshot_id: i64,
    /// Deterministically ordered candidates from that snapshot.
    pub candidates: Vec<ForgePlanCandidate>,
}

/// Deterministic owner of Forge's capacity-scaled executable envelope policy.
pub struct ForgeEnvelopeSizer;

impl ForgeEnvelopeSizer {
    /// Builds the largest supported resident quantum and exact scratch siblings.
    ///
    /// # Errors
    /// Returns a typed capacity refusal when no complete envelope fits and an
    /// invariant error when checked arithmetic cannot represent the terms.
    pub fn size(
        total_input_bytes: u64,
        file_count: usize,
        max_concurrent_reads: usize,
        capacity: ForgeCapacity,
    ) -> Result<ForgeTaskEnvelope, ForgeError> {
        const MIB: u64 = 1024 * 1024;
        const FOOTER_ENCODED_BYTES: u64 = 8 * MIB;
        const FOOTER_DECODE_WORKSPACE_BYTES: u64 = 32 * MIB;
        if file_count == 0 || max_concurrent_reads == 0 || capacity.max_spill_bytes < 2 * MIB {
            return Err(ForgeError::Capacity {
                detail: "Forge topology cannot supply a complete resource envelope".to_owned(),
            });
        }
        let scratch_term = total_input_bytes
            .max(
                super::rewrite::REWRITE_WORKING_SET_FLOOR_BYTES
                    .checked_mul(16)
                    .ok_or_else(|| ForgeError::Invariant {
                        detail: "Forge scratch floor overflows".to_owned(),
                    })?,
            )
            .min(capacity.max_spill_bytes / 2);
        if scratch_term < MIB || (file_count == 1 && total_input_bytes > scratch_term) {
            return Err(ForgeError::Capacity {
                detail: "Forge input cannot fit the bounded sort-spill reservation".to_owned(),
            });
        }
        for decoded_batch_bytes in [32 * MIB, 16 * MIB, 8 * MIB, 4 * MIB, 2 * MIB, MIB] {
            let encoder_buffer_bytes =
                decoded_batch_bytes
                    .checked_mul(2)
                    .ok_or_else(|| ForgeError::Invariant {
                        detail: "Forge encoder term overflows".to_owned(),
                    })?;
            let upload_chunk_bytes = (8 * MIB).min(decoded_batch_bytes);
            let sort_merge_reservation_bytes = (10 * MIB).min(decoded_batch_bytes);
            let sort_working_bytes = decoded_batch_bytes
                .checked_mul(2)
                .and_then(|value| value.checked_add(sort_merge_reservation_bytes))
                .ok_or_else(|| ForgeError::Invariant {
                    detail: "Forge sort term overflows".to_owned(),
                })?;
            let fixed = sort_working_bytes
                .checked_add(encoder_buffer_bytes)
                .and_then(|value| value.checked_add(upload_chunk_bytes))
                .and_then(|value| value.checked_add(FOOTER_ENCODED_BYTES))
                .ok_or_else(|| ForgeError::Invariant {
                    detail: "Forge fixed resident terms overflow".to_owned(),
                })?;
            if fixed >= capacity.max_memory_bytes {
                continue;
            }
            let available_readers = (capacity.max_memory_bytes - fixed) / decoded_batch_bytes;
            let readers = available_readers
                .min(u64::from(capacity.max_parallelism))
                .min(u64::try_from(max_concurrent_reads).unwrap_or(u64::MAX))
                .min(u64::try_from(file_count).unwrap_or(u64::MAX));
            if readers == 0 {
                continue;
            }
            let reader_permits = u16::try_from(readers).map_err(|_| ForgeError::Invariant {
                detail: "Forge reader count exceeds u16".to_owned(),
            })?;
            let envelope = ForgeTaskEnvelope {
                version: FORGE_ENVELOPE_VERSION,
                reader_permits,
                decoded_batch_bytes,
                decoded_input_bytes: readers.checked_mul(decoded_batch_bytes).ok_or_else(|| {
                    ForgeError::Invariant {
                        detail: "Forge decoded term overflows".to_owned(),
                    }
                })?,
                sort_working_bytes,
                sort_merge_reservation_bytes,
                encoder_buffer_bytes,
                upload_chunk_bytes,
                footer_encoded_bytes: FOOTER_ENCODED_BYTES,
                footer_decode_workspace_bytes: FOOTER_DECODE_WORKSPACE_BYTES,
                sort_spill_bytes: scratch_term,
            };
            envelope.validate().map_err(|error| ForgeError::Invariant {
                detail: error.to_string(),
            })?;
            return Ok(envelope);
        }
        Err(ForgeError::Capacity {
            detail: "Forge resident capacity cannot fit the minimum quantum".to_owned(),
        })
    }
}

/// Closed capacity result assigned exactly once to every candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgePlanCapacity {
    /// The plan fits every ordinary ceiling.
    Ordinary,
    /// One task fits every non-byte ceiling and requires the per-worker large lane.
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

    /// Validates one candidate's resource estimates against this planner's ceilings.
    ///
    /// This is the shared admission predicate for both the durable plan path and
    /// the direct live-replacement path: memory and spill estimates must be
    /// positive and must not exceed the already validated [`ForgeCapacity`]
    /// ceilings. It never clamps, inflates, or otherwise rewrites an estimate.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Capacity`] when either estimate is zero or exceeds
    /// its ceiling.
    pub(crate) fn validate_candidate_estimates(
        &self,
        memory_bytes: u64,
        spill_bytes: u64,
    ) -> Result<(), ForgeError> {
        if memory_bytes == 0 || spill_bytes == 0 {
            return Err(ForgeError::Capacity {
                detail: "Forge rewrite demand must be positive".to_owned(),
            });
        }
        if memory_bytes > self.capacity.max_memory_bytes
            || spill_bytes > self.capacity.max_spill_bytes
        {
            return Err(ForgeError::Capacity {
                detail: "Forge rewrite demand exceeds its validated capacity ceiling".to_owned(),
            });
        }
        Ok(())
    }

    /// Plans every candidate discovered from one immutable snapshot.
    ///
    /// Candidates are already the exact work a discovery pass selected, so the
    /// planner neither regroups nor splits them: it binds each one to the
    /// snapshot identity, hashes its canonical payload, and assigns the single
    /// capacity classification that decides its lane.
    ///
    /// # Errors
    /// Returns [`ForgeError::Invariant`] when a candidate's inputs or estimates
    /// are empty, unsorted, duplicated, or misaligned, and
    /// [`ForgeError::Capacity`] when the executable envelope cannot be sized.
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

    /// Binds one candidate to its snapshot and classifies its capacity.
    ///
    /// The canonical hash covers exactly the persisted payload — inputs,
    /// parameters, and payload version — so two discovery passes that select
    /// the same work produce the same durable identity and the idempotent
    /// enqueue collapses them.
    ///
    /// # Errors
    /// Returns [`ForgeError::Invariant`] when the input count exceeds `u32`,
    /// any estimate is zero, the per-input byte terms do not align with the
    /// inputs, the inputs are not strictly sorted, or the payload cannot be
    /// canonicalized; returns [`ForgeError::Capacity`] when the envelope for the
    /// candidate cannot be sized within this planner's ceilings.
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
            || candidate.inputs.len() != candidate.input_bytes.len()
            || candidate.inputs.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(ForgeError::Invariant {
                detail: "Forge candidate inputs and estimates must be positive, sorted, and unique"
                    .to_owned(),
            });
        }
        let capacity_outcome = if candidate.bytes <= self.capacity.max_large_task_bytes
            && candidate.parallelism <= self.capacity.max_parallelism
            && candidate.memory_bytes <= self.capacity.max_memory_bytes
            && candidate.spill_bytes <= self.capacity.max_spill_bytes
        {
            ForgePlanCapacity::Ordinary
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
                envelope: Some(ForgeEnvelopeSizer::size(
                    candidate.bytes,
                    candidate.inputs.len(),
                    usize::from(candidate.parallelism),
                    self.capacity,
                )?),
            },
            capacity: capacity_outcome,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sizer chooses the largest batch quantum before maximizing readers.
    #[test]
    fn envelope_sizer_selects_largest_batch_then_readers() {
        let mib = 1024 * 1024;
        for (memory, expected_batch, expected_readers) in [
            (64 * mib, 8 * mib, 1),
            (128 * mib, 16 * mib, 2),
            (256 * mib, 32 * mib, 3),
        ] {
            let envelope = ForgeEnvelopeSizer::size(
                1,
                4,
                4,
                ForgeCapacity {
                    max_parallelism: 4,
                    max_memory_bytes: memory,
                    max_spill_bytes: 1024 * mib,
                    max_large_task_bytes: u64::MAX,
                },
            )
            .expect("supported envelope");
            assert_eq!(envelope.decoded_batch_bytes, expected_batch);
            assert_eq!(envelope.reader_permits, expected_readers);
            assert_eq!(
                envelope.decoded_input_bytes,
                u64::from(expected_readers) * expected_batch
            );
            assert!(envelope.memory_bytes().expect("resident total") <= memory);
        }
    }

    /// The sizer refuses before allocation when even its one-MiB quantum cannot fit.
    #[test]
    fn envelope_rejects_when_minimum_quantum_cannot_fit() {
        let mib = 1024 * 1024;
        let result = ForgeEnvelopeSizer::size(
            1,
            1,
            1,
            ForgeCapacity {
                max_parallelism: 1,
                max_memory_bytes: 6 * mib,
                max_spill_bytes: 2 * mib,
                max_large_task_bytes: u64::MAX,
            },
        );

        assert!(matches!(result, Err(ForgeError::Capacity { .. })));
    }

    /// Sort spill is the sole scratch term, capacity-bounded, and refuses incomplete topology.
    #[test]
    fn envelope_scratch_terms_are_capacity_bounded() {
        let mib = 1024 * 1024;
        let capacity = ForgeCapacity {
            max_parallelism: 4,
            max_memory_bytes: 256 * mib,
            max_spill_bytes: 1024 * mib,
            max_large_task_bytes: u64::MAX,
        };
        let envelope = ForgeEnvelopeSizer::size(1, 4, 4, capacity).expect("bounded scratch");
        assert_eq!(envelope.sort_spill_bytes, 512 * mib);
        assert_eq!(envelope.scratch_bytes().expect("scratch total"), 512 * mib);
        assert!(
            ForgeEnvelopeSizer::size(
                1,
                1,
                1,
                ForgeCapacity {
                    max_spill_bytes: 2 * mib - 1,
                    ..capacity
                }
            )
            .is_err()
        );
        assert!(ForgeEnvelopeSizer::size(513 * mib, 1, 1, capacity).is_err());
    }
}
