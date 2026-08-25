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
    /// Maximum bytes for one large-lane singleton admitted under a worker's own
    /// per-worker capacity budget.
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

impl ForgePlanCandidate {
    /// Derives the one exact candidate a live rewrite group would be planned as.
    ///
    /// This is the sole candidate-estimation algorithm for live rewrite groups.
    /// The durable planning scheduler and the `test-support` direct replacement
    /// path both call it, so a directly replaced group produces byte-identical
    /// estimates to the task the planner would have persisted for that group.
    /// Input identities are sorted and deduplicated, and selected input bytes
    /// are checked-summed.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when selected input bytes overflow
    /// `u64`, concurrency is zero, or the bounded read width exceeds the
    /// durable parallelism domain.
    pub(crate) fn from_live_group(
        group: &super::right_size::IcebergRewriteGroup,
        max_concurrent_reads: usize,
        capacity: ForgeCapacity,
    ) -> Result<Self, ForgeError> {
        if max_concurrent_reads == 0 {
            return Err(ForgeError::Invariant {
                detail: "Forge live-group concurrency must be positive".to_owned(),
            });
        }
        let mut input_terms = group
            .files()
            .iter()
            .map(|file| (file.catalog_path().to_owned(), file.file_size_bytes()))
            .collect::<Vec<_>>();
        input_terms.sort_by(|left, right| left.0.cmp(&right.0));
        input_terms.dedup_by(|left, right| left.0 == right.0);
        let (inputs, input_bytes): (Vec<_>, Vec<_>) = input_terms.into_iter().unzip();
        let bytes = group
            .files()
            .iter()
            .try_fold(0_u64, |total, file| {
                total.checked_add(file.file_size_bytes())
            })
            .ok_or_else(|| ForgeError::Invariant {
                detail: "Forge group bytes overflow".to_owned(),
            })?;
        let envelope =
            ForgeEnvelopeSizer::size(bytes, inputs.len(), max_concurrent_reads, capacity)?;
        Ok(Self {
            strategy: ForgeTaskStrategy::SmallFiles,
            parallelism: envelope
                .reader_permits
                .min(u16::try_from(group.files().len()).unwrap_or(u16::MAX))
                .max(1),
            memory_bytes: envelope
                .memory_bytes()
                .map_err(|error| ForgeError::Invariant {
                    detail: error.to_string(),
                })?,
            spill_bytes: envelope
                .scratch_bytes()
                .map_err(|error| ForgeError::Invariant {
                    detail: error.to_string(),
                })?,
            inputs,
            input_bytes,
            bytes,
            parameters: serde_json::json!({"kind":"live_rewrite"}),
        })
    }
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

    /// Plans every candidate from one stable snapshot and assigns one capacity outcome.
    ///
    /// # Errors
    /// Returns an invariant error for empty, unsorted, duplicate, or zero-valued candidates.
    pub fn plan_table(
        &self,
        snapshot: &ForgeTableSnapshot,
    ) -> Result<Vec<PlannedForgeTask>, ForgeError> {
        let mut planned = Vec::new();
        for candidate in &snapshot.candidates {
            for split in self.split_candidate(candidate)? {
                planned.push(self.plan_candidate(snapshot.snapshot_id, &split)?);
            }
        }
        Ok(planned)
    }

    /// Splits an oversized multi-file candidate into stable contiguous fitting groups.
    ///
    /// A singleton is preserved for ordinary/large-lane classification. Splits
    /// retain sorted path order and recompute the streaming envelope from each
    /// group's exact bytes, so total input never becomes resident memory.
    ///
    /// # Errors
    /// Returns an invariant error when per-input sizes are missing or misaligned.
    fn split_candidate(
        &self,
        candidate: &ForgePlanCandidate,
    ) -> Result<Vec<ForgePlanCandidate>, ForgeError> {
        if candidate.inputs.len() != candidate.input_bytes.len()
            || candidate.input_bytes.contains(&0)
        {
            return Err(ForgeError::Invariant {
                detail: "Forge candidate paths and input byte terms must align".to_owned(),
            });
        }
        let group_byte_ceiling = self
            .capacity
            .max_bytes
            .min(self.capacity.max_spill_bytes / 2);
        if candidate.inputs.len() <= 1
            || (candidate.inputs.len() <= self.capacity.max_files as usize
                && candidate.bytes <= group_byte_ceiling)
        {
            return Ok(vec![candidate.clone()]);
        }
        let mut groups = Vec::new();
        let mut start = 0;
        while start < candidate.inputs.len() {
            let mut end = start;
            let mut bytes = 0_u64;
            while end < candidate.inputs.len()
                && end - start < self.capacity.max_files as usize
                && bytes.saturating_add(candidate.input_bytes[end]) <= group_byte_ceiling
            {
                bytes = bytes.saturating_add(candidate.input_bytes[end]);
                end += 1;
            }
            if end == start {
                end += 1;
                bytes = candidate.input_bytes[start];
            }
            let envelope = ForgeEnvelopeSizer::size(
                bytes,
                end - start,
                usize::from(candidate.parallelism).min(end - start),
                self.capacity,
            )?;
            groups.push(ForgePlanCandidate {
                strategy: candidate.strategy,
                inputs: candidate.inputs[start..end].to_vec(),
                input_bytes: candidate.input_bytes[start..end].to_vec(),
                bytes,
                parallelism: envelope.reader_permits,
                memory_bytes: envelope
                    .memory_bytes()
                    .map_err(|error| ForgeError::Invariant {
                        detail: error.to_string(),
                    })?,
                spill_bytes: envelope
                    .scratch_bytes()
                    .map_err(|error| ForgeError::Invariant {
                        detail: error.to_string(),
                    })?,
                parameters: candidate.parameters.clone(),
            });
            start = end;
        }
        Ok(groups)
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
            || candidate.inputs.len() != candidate.input_bytes.len()
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
                    max_files: 4,
                    max_bytes: u64::MAX,
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
                max_files: 1,
                max_bytes: u64::MAX,
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
            max_files: 4,
            max_bytes: u64::MAX,
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

        let input_bytes = vec![400 * mib, 400 * mib];
        let candidate = ForgePlanCandidate {
            strategy: ForgeTaskStrategy::SmallFiles,
            inputs: vec!["a.parquet".to_owned(), "b.parquet".to_owned()],
            input_bytes,
            bytes: 800 * mib,
            parallelism: envelope.reader_permits,
            memory_bytes: envelope.memory_bytes().expect("resident total"),
            spill_bytes: envelope.scratch_bytes().expect("scratch total"),
            parameters: serde_json::json!({}),
        };
        let planned = ForgePlanner::new(capacity)
            .plan_table(&ForgeTableSnapshot {
                snapshot_id: 1,
                candidates: vec![candidate],
            })
            .expect("scratch-bound input splits");
        assert_eq!(planned.len(), 2);
        assert!(planned.iter().all(|task| task.estimates.bytes == 400 * mib));
    }

    /// Returns capacity with independently testable ceilings.
    fn capacity() -> ForgeCapacity {
        ForgeCapacity {
            max_files: 4,
            max_bytes: 100,
            max_parallelism: 2,
            max_memory_bytes: 128 * 1024 * 1024,
            max_spill_bytes: 2 * 1024 * 1024 * 1024,
            max_large_task_bytes: 200,
        }
    }

    /// Returns one valid candidate whose selected estimate can be changed.
    fn candidate() -> ForgePlanCandidate {
        ForgePlanCandidate {
            strategy: ForgeTaskStrategy::SmallFiles,
            inputs: vec!["a.parquet".to_owned()],
            input_bytes: vec![50],
            bytes: 50,
            parallelism: 1,
            memory_bytes: 100 * 1024 * 1024,
            spill_bytes: 1024 * 1024 * 1024,
            parameters: serde_json::json!({"target":64}),
        }
    }

    /// Every capacity dimension independently controls the sole outcome.
    #[test]
    fn planner_classifies_all_capacity_dimensions_and_singleton_lane() {
        let owner = ForgePlanner::new(capacity());
        for mutate in [
            |c: &mut ForgePlanCandidate| c.bytes = 201,
            |c: &mut ForgePlanCandidate| c.parallelism = 3,
            |c: &mut ForgePlanCandidate| c.memory_bytes = 129 * 1024 * 1024,
            |c: &mut ForgePlanCandidate| c.spill_bytes = 2 * 1024 * 1024 * 1024 + 1,
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
        multi_file_overflow.input_bytes = vec![75, 75];
        multi_file_overflow.bytes = 150;
        let tasks = owner
            .plan_table(&ForgeTableSnapshot {
                snapshot_id: 7,
                candidates: vec![multi_file_overflow],
            })
            .expect("multi-file overflow plan");
        assert_eq!(tasks.len(), 2);
        assert!(
            tasks
                .iter()
                .all(|task| task.capacity == ForgePlanCapacity::Ordinary)
        );
        assert_ne!(tasks[0].plan_hash, tasks[1].plan_hash);
    }

    /// A production-shaped 51-file group is admitted from bounded streaming terms.
    #[test]
    fn fifty_one_file_group_schedules_with_bounded_streaming_envelope() {
        let capacity = ForgeCapacity {
            max_files: 64,
            max_bytes: 4 * 1024 * 1024 * 1024,
            max_parallelism: 4,
            max_memory_bytes: 512 * 1024 * 1024,
            max_spill_bytes: 8 * 1024 * 1024 * 1024,
            max_large_task_bytes: 8 * 1024 * 1024 * 1024,
        };
        let input_bytes = vec![64 * 1024 * 1024; 51];
        let bytes = input_bytes.iter().sum();
        let envelope = ForgeEnvelopeSizer::size(bytes, 51, 4, capacity).expect("51-file envelope");
        let candidate = ForgePlanCandidate {
            strategy: ForgeTaskStrategy::SmallFiles,
            inputs: (0..51).map(|index| format!("{index:02}.parquet")).collect(),
            input_bytes,
            bytes,
            parallelism: envelope.reader_permits,
            memory_bytes: envelope.memory_bytes().expect("resident total"),
            spill_bytes: envelope.scratch_bytes().expect("scratch total"),
            parameters: serde_json::json!({"kind":"live_rewrite"}),
        };
        let first = ForgePlanner::new(capacity)
            .plan_table(&ForgeTableSnapshot {
                snapshot_id: 9,
                candidates: vec![candidate.clone()],
            })
            .expect("51-file group plans");
        let second = ForgePlanner::new(capacity)
            .plan_table(&ForgeTableSnapshot {
                snapshot_id: 9,
                candidates: vec![candidate],
            })
            .expect("repeat plan is deterministic");
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].capacity, ForgePlanCapacity::Ordinary);
        assert_eq!(first[0].plan_hash, second[0].plan_hash);
        assert!(first[0].estimates.memory_bytes < first[0].estimates.bytes);
    }

    /// Live-group admission uses execution concurrency rather than file count.
    #[test]
    fn live_group_parallelism_is_bounded_by_concurrent_reads() {
        let group = super::super::right_size::IcebergRewriteGroup {
            files: (0..20)
                .map(|index| {
                    super::super::right_size::candidate_file_for_test(
                        &format!("{index:02}.parquet"),
                        10,
                    )
                })
                .collect(),
            reason: super::super::right_size::IcebergRewriteReason::Undersized,
        };
        let candidate = ForgePlanCandidate::from_live_group(
            &group,
            4,
            ForgeCapacity {
                max_files: 64,
                max_bytes: u64::MAX,
                max_parallelism: 4,
                max_memory_bytes: 512 * 1024 * 1024,
                max_spill_bytes: 1024 * 1024 * 1024,
                max_large_task_bytes: u64::MAX,
            },
        )
        .expect("bounded live group must produce a candidate");
        assert_eq!(candidate.parallelism, 4);
    }

    /// Live-group persisted readers equal the readers used to size decoded memory.
    #[test]
    fn live_group_parallelism_scales_to_memory_capacity() {
        let group = super::super::right_size::IcebergRewriteGroup {
            files: (0..8)
                .map(|index| {
                    super::super::right_size::candidate_file_for_test(
                        &format!("{index:02}.parquet"),
                        10,
                    )
                })
                .collect(),
            reason: super::super::right_size::IcebergRewriteReason::Undersized,
        };
        let capacity = ForgeCapacity {
            max_files: 64,
            max_bytes: u64::MAX,
            max_parallelism: 8,
            max_memory_bytes: 64 * 1024 * 1024,
            max_spill_bytes: 1024 * 1024 * 1024,
            max_large_task_bytes: u64::MAX,
        };
        let candidate = ForgePlanCandidate::from_live_group(&group, 8, capacity)
            .expect("constrained live group candidate");
        let envelope = ForgeEnvelopeSizer::size(candidate.bytes, 8, 8, capacity)
            .expect("constrained envelope");
        assert_eq!(candidate.parallelism, envelope.reader_permits);
        assert_eq!(
            candidate.memory_bytes,
            envelope.memory_bytes().expect("resident total")
        );
    }

    /// Configuration conversion copies every field and rejects invalid relationships.
    #[test]
    fn capacity_conversion_is_field_complete_and_validated() {
        let mut config = ForgeConfig {
            max_files_per_tick: 17,
            max_bytes_per_tick: 101,
            max_concurrent_reads: 3,
            max_memory_bytes: 103,
            spill_limit_bytes: 107,
            max_large_task_bytes: 109,
            ..ForgeConfig::default()
        };
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
