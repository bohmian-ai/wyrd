//! Peak-memory estimation for one planned compaction group.
//!
redacted
//! `origin/main` `6f8fbbfd06d25d195bdff9a4f1cb246cf4363903`, file
//! `src/storage/src/hummock/compactor/iceberg_compaction/memory.rs`
//! (lines 21-468). Licensed Apache-2.0, Copyright `RisingWave` Labs.
//!
//! Only imports and visibility are adapted; every constant, branch, and
//! arithmetic operation is byte-for-byte upstream. Forge admits plans against
redacted
//! fits here, and drift between the two is a code change rather than a silent
//! behavioral difference. The estimator is pure and synchronous: it reads the
//! plan, the table schema, and three configuration facts, and touches no IO,
//! no lease, and no catalog.

// Copyright 2026 RisingWave Labs
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::mem::size_of;

use iceberg::scan::FileScanTask;
use iceberg::spec::{DataContentType, FormatVersion, PrimitiveType, Schema, Type};
use iceberg_compaction_core::compaction::CompactionPlan;
use num_traits::ToPrimitive;

/// Widens an unsigned 64-bit count to the pointer-sized arithmetic the
/// estimator sums in.
///
/// Every caller is a file size or record count that upstream widens with a
/// plain cast. On the 64-bit targets Bifrost supports the conversion is exact;
/// the saturation only exists so a hypothetical 32-bit target degrades to the
/// largest representable estimate — which the queue then refuses as too large —
/// instead of wrapping to a small one that would be admitted.
fn widen(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

/// Converts a byte or record count to the float the ratio arithmetic uses.
///
/// The conversion is total for every `usize`, so the fallback is unreachable;
/// it is written rather than asserted because an estimate is not worth a panic.
/// Precision loss above 2^53 is upstream's and is irrelevant at that magnitude:
/// the resulting estimate is refused as too large either way.
fn as_f64(value: usize) -> f64 {
    value.to_f64().unwrap_or(f64::MAX)
}

/// Converts a scaled float estimate back to bytes with saturating semantics.
///
/// Matches the primitive cast upstream uses: a negative or NaN scale becomes
/// zero and an overflowing one becomes the largest representable estimate, so
/// no scale can produce a small figure the queue would wrongly admit.
fn as_usize(value: f64) -> usize {
    if value.is_nan() || value <= 0.0 {
        return 0;
    }
    value.to_usize().unwrap_or(usize::MAX)
}

/// `DataFusion`'s `datafusion.execution.sort_spill_reservation_bytes` default.
///
/// Each `ExternalSorter` partition resizes a merge reservation to this value before it sorts a
/// single row. Account for it even when `DataFusion` uses its default unbounded memory pool.
const DATAFUSION_SORT_MERGE_RESERVATION_BYTES: usize = 10 * 1024 * 1024;

/// Fixed task-context, operator, and allocator overhead observed for small streaming plans.
const DATAFUSION_RUNTIME_FIXED_BYTES: usize = 640 * 1024;

/// Upper bound for decoded Arrow data retained beyond the explicitly accounted record batches.
const DATAFUSION_STREAMING_DECODED_WINDOW_BYTES: usize = 16 * 1024 * 1024;

/// Output writer memory retained across input batches.
const DATAFUSION_WRITER_WINDOW_BYTES: usize = 10 * 1024 * 1024;

const LARGE_SORT_THRESHOLD_BYTES: usize = 64 * 1024 * 1024;
const SORT_TEMPORARY_HEADROOM_BYTES: usize = 32 * 1024 * 1024;
const HEAP_FIXED_HEADROOM_BYTES: usize = 256 * 1024;

/// Compressed-to-decoded fallback used when the schema contains variable-width fields.
const COMPRESSED_TO_DECODED_INFLATION: usize = 4;

/// At this scan concurrency, prefetched files and decoded buffers measurably overlap at peak.
const STREAMING_PREFETCH_DECODE_OVERLAP_MIN_ACTIVE_FILES: usize = 4;

/// Compressed-size estimate for decoded equality-delete value buffers.
const EQUALITY_DELETE_INFLATION: usize = 8;

/// Compressed-size estimate for decoded position-delete value buffers.
const POSITION_DELETE_INFLATION: usize = 5;

/// Per-row hash table and join bookkeeping floor for `DataFusion`'s retained `HashJoinInput`.
const HASH_JOIN_ROW_OVERHEAD_BYTES: usize = 40;

/// Per-row Arrow and hashing overhead for the two position-delete join keys.
const POSITION_DELETE_KEY_OVERHEAD_BYTES: usize = 32;

/// The plan facts the `DataFusion` pool estimate is derived from.
///
/// They are grouped because the sorted and streaming branches read overlapping
/// but different subsets of them, and threading eleven positional arguments
/// through would make the branch that reads each one unreadable.
struct PoolInputs {
    /// Number of data files the plan reads.
    data_file_count: usize,
    /// Parallelism the plan recommends for its scan side.
    executor_parallelism: usize,
    /// Compressed bytes across every input data file.
    compressed_input_bytes: usize,
    /// Decoded bytes the inputs expand to.
    decoded_input_bytes: usize,
    /// Bytes staged by prefetch, or zero when prefetch is disabled.
    prefetch: usize,
    /// Bytes the broadcast equality-delete side materialises.
    equality_delete_bytes: usize,
    /// Bytes the position-delete anti-join hash table holds.
    position_delete_join_bytes: usize,
    /// Tracked sort workspace, or zero for a streaming plan.
    sort_workspace_bytes: usize,
    /// Merge reservation pinned per sorted output partition.
    sort_merge_headroom_bytes: usize,
    /// One record batch's allocation.
    batch_allocation_bytes: usize,
    /// Concurrent batch allocation across every scan partition.
    batch_overhead_bytes: usize,
}

/// Returns the retained-operator bytes and the `DataFusion` pool peak.
///
/// The two branches are upstream's and are not interchangeable: a sorted plan
/// peaks when the sorter holds its full workspace beside the reservations it
/// pinned before reading, while a streaming plan never exceeds what it retains,
/// so its peak and its retention are the same figure.
fn datafusion_pool_bytes(requires_sort: bool, inputs: &PoolInputs) -> (usize, usize) {
    if requires_sort {
        let retained_operator_bytes = inputs
            .sort_merge_headroom_bytes
            .saturating_add(inputs.equality_delete_bytes)
            .saturating_add(inputs.position_delete_join_bytes)
            .saturating_add(inputs.prefetch)
            .saturating_add(inputs.batch_overhead_bytes);
        let sort_peak = inputs
            .sort_workspace_bytes
            .saturating_add(inputs.sort_merge_headroom_bytes)
            .saturating_add(inputs.equality_delete_bytes)
            .saturating_add(inputs.position_delete_join_bytes)
            .saturating_add(inputs.prefetch);
        (
            retained_operator_bytes,
            sort_peak.max(retained_operator_bytes),
        )
    } else {
        let data_file_count = inputs.data_file_count.max(1);
        let active_files = inputs.executor_parallelism.min(inputs.data_file_count);
        // Bound the extra decoder window from two directions: one partition's proportional share
        // of the decoded input, and the active files' fair share with 25% pipeline headroom.
        let per_partition_share = inputs
            .decoded_input_bytes
            .checked_div(active_files.max(1))
            .unwrap_or(inputs.decoded_input_bytes);
        let active_file_share = inputs
            .decoded_input_bytes
            .checked_div(data_file_count)
            .unwrap_or(inputs.decoded_input_bytes)
            .saturating_mul(active_files);
        let active_file_share_with_headroom =
            active_file_share.saturating_add(active_file_share / 4);
        let decoded_in_flight = per_partition_share
            .min(active_file_share_with_headroom)
            .min(DATAFUSION_STREAMING_DECODED_WINDOW_BYTES);
        let overlap_input_threshold =
            DATAFUSION_STREAMING_DECODED_WINDOW_BYTES / COMPRESSED_TO_DECODED_INFLATION;
        let scan_buffer_bytes = if active_files
            >= STREAMING_PREFETCH_DECODE_OVERLAP_MIN_ACTIVE_FILES
            && inputs.compressed_input_bytes >= overlap_input_threshold
        {
            inputs.prefetch.saturating_add(decoded_in_flight)
        } else {
            inputs.prefetch.max(decoded_in_flight)
        };
        let common_bytes = scan_buffer_bytes
            .saturating_add(inputs.batch_overhead_bytes)
            .saturating_add(inputs.equality_delete_bytes);
        let retained_operator_bytes = common_bytes
            .saturating_add(inputs.position_delete_join_bytes)
            .max(inputs.batch_allocation_bytes)
            .max(DATAFUSION_RUNTIME_FIXED_BYTES);
        (retained_operator_bytes, retained_operator_bytes)
    }
}

/// The plan facts the actual-heap estimate is derived from.
///
/// Grouped for the same reason as [`PoolInputs`]: the sorted, streaming, and
/// delete-join phases each read a different subset, and fifteen positional
/// arguments would hide which phase reads which.
struct HeapInputs {
    /// Number of data files the plan reads.
    data_file_count: usize,
    /// Parallelism the plan recommends for its scan side.
    executor_parallelism: usize,
    /// Parallelism the plan recommends for its write side.
    output_parallelism: usize,
    /// Rows across every input data file that reported a count.
    record_count: usize,
    /// Configured maximum rows per record batch.
    max_record_batch_rows: usize,
    /// Per-row bytes of the hidden columns a delete join retains.
    hidden_row_width: usize,
    /// Fixed per-row width, or `None` for a variable-width schema.
    schema_row_width: Option<usize>,
    /// Decoded bytes the inputs expand to.
    decoded_input_bytes: usize,
    /// Decoded bytes the outputs expand to.
    decoded_output_bytes: usize,
    /// Bytes the broadcast equality-delete side materialises.
    equality_delete_bytes: usize,
    /// Raw bytes of the position-delete files, before the join factor.
    position_delete_raw_bytes: usize,
    /// One record batch's allocation.
    batch_allocation_bytes: usize,
    /// Concurrent batch allocation across every scan partition.
    batch_overhead_bytes: usize,
    /// Bytes the pool estimate says the plan retains.
    retained_operator_bytes: usize,
    /// The pool peak the plan estimate reached.
    estimated_datafusion_peak_bytes: usize,
}

/// Bytes a delete join retains beside the scan phase.
///
/// A position delete scales the retained hidden columns by how much of the
/// scan it actually overlaps, then adds the raw delete bytes the anti-join
/// hash table expands to. An equality delete is broadcast instead, so it
/// retains its whole materialised side. A plan with neither retains nothing.
fn delete_join_heap_bytes(plan: &CompactionPlan, inputs: &HeapInputs) -> usize {
    // Delete joins retain hidden probe columns while the build side remains materialized.
    let hidden_total_bytes = inputs.hidden_row_width.saturating_mul(inputs.record_count);
    let position_delete_records = widen(
        plan.file_group
            .position_delete_files
            .iter()
            .filter_map(|task| task.record_count)
            .fold(0u64, u64::saturating_add),
    );
    if position_delete_records > 0 {
        let delete_ratio = as_f64(position_delete_records) / as_f64(inputs.record_count.max(1));
        let overlap_factor = 0.6 + 0.3 * delete_ratio.min(1.0);
        return as_usize(as_f64(hidden_total_bytes) * overlap_factor)
            + inputs.position_delete_raw_bytes.saturating_mul(8);
    }
    if plan.file_group.equality_delete_files.is_empty() {
        return 0;
    }
    hidden_total_bytes.saturating_add(inputs.equality_delete_bytes)
}

/// Returns the plan's estimated peak actual heap, with its fixed headroom.
///
/// This is the half of the estimate that is not `DataFusion`'s logical pool:
/// the `OpenDAL` input buffers, the decode window, the writer window, and the
/// delete-join retention. The pool estimate enters it only as a floor, because
/// with an unbounded pool the pool figure is an accounting of overlap rather
/// than a limit anything is allocated against.
fn heap_peak_bytes(
    plan: &CompactionPlan,
    format_version: FormatVersion,
    requires_sort: bool,
    inputs: &HeapInputs,
) -> usize {
    let data_file_count = inputs.data_file_count.max(1);
    let active_data_files = inputs.executor_parallelism.min(inputs.data_file_count);
    let active_decoded_bytes = inputs
        .decoded_input_bytes
        .checked_div(data_file_count)
        .unwrap_or(inputs.decoded_input_bytes)
        .saturating_mul(active_data_files);
    let active_input_bytes = estimate_prefetch_bytes(plan, format_version);
    let total_batches = inputs
        .record_count
        .saturating_add(inputs.max_record_batch_rows.saturating_sub(1))
        .checked_div(inputs.max_record_batch_rows)
        .unwrap_or_default();
    let batch_overlap =
        (as_f64(total_batches) / as_f64(inputs.executor_parallelism.saturating_mul(8))).min(1.0);
    let batch_heap_bytes = as_usize(as_f64(inputs.batch_overhead_bytes) * batch_overlap);
    // OpenDAL S3 buffers the active compressed input while Parquet decoding and scan batches
    // overlap. This phase is independent from DataFusion's logical pool reservations.
    let scan_heap_bytes = active_input_bytes
        .saturating_add(active_input_bytes.min(DATAFUSION_STREAMING_DECODED_WINDOW_BYTES))
        .saturating_add(active_decoded_bytes.min(DATAFUSION_STREAMING_DECODED_WINDOW_BYTES))
        .saturating_add(batch_heap_bytes);
    let writer_heap_bytes = if inputs.schema_row_width.is_some() {
        inputs.decoded_output_bytes.saturating_mul(3) / 2
    } else {
        let output_scale = as_f64(inputs.output_parallelism.min(4)) / 4.0;
        let large_single_scan_scale = if inputs.executor_parallelism == 1 {
            (as_f64(active_input_bytes) / as_f64(DATAFUSION_STREAMING_DECODED_WINDOW_BYTES))
                .min(1.0)
        } else {
            0.0
        };
        as_usize(
            as_f64((inputs.decoded_output_bytes / 2).min(DATAFUSION_WRITER_WINDOW_BYTES))
                * output_scale.max(large_single_scan_scale),
        )
    }
    .min(DATAFUSION_WRITER_WINDOW_BYTES);
    let streaming_heap_bytes = scan_heap_bytes.saturating_add(writer_heap_bytes);

    let join_heap_bytes = delete_join_heap_bytes(plan, inputs);
    // Large sorts retain their full decoded input. Smaller sorts release part of each partition as
    // merge runs, based on the S3 heap profiles used to calibrate this estimate.
    let sorted_decoded_bytes = if inputs.decoded_output_bytes > 64 * 1024 * 1024 {
        inputs.decoded_output_bytes
    } else {
        inputs.decoded_output_bytes.saturating_mul(2) / 3
    };
    let sorted_heap_bytes = active_input_bytes
        .saturating_add(active_input_bytes.min(DATAFUSION_STREAMING_DECODED_WINDOW_BYTES))
        .saturating_add(sorted_decoded_bytes);
    let execution_heap_bytes = if requires_sort {
        sorted_heap_bytes
    } else {
        streaming_heap_bytes
    };
    let join_phase_bytes = scan_heap_bytes.saturating_add(join_heap_bytes);
    let join_phase_bytes = if requires_sort {
        join_phase_bytes.saturating_mul(3) / 4
    } else {
        join_phase_bytes
    };
    let large_sorted = requires_sort && inputs.decoded_output_bytes > LARGE_SORT_THRESHOLD_BYTES;
    let fixed_heap_peak_bytes = if large_sorted {
        scan_heap_bytes.max(join_phase_bytes)
    } else {
        execution_heap_bytes.max(join_phase_bytes)
    };
    let datafusion_peak_with_headroom = inputs
        .estimated_datafusion_peak_bytes
        .saturating_add(inputs.batch_allocation_bytes)
        .saturating_add(if requires_sort {
            DATAFUSION_STREAMING_DECODED_WINDOW_BYTES
        } else {
            0
        });
    // Preserve a progress-phase floor in the estimate. With an unbounded pool this is not an
    // allocation limit; it accounts for decoded input that can overlap while the pipeline fills.
    let datafusion_progress_peak_bytes = if large_sorted {
        inputs
            .retained_operator_bytes
            .saturating_add(active_decoded_bytes.saturating_mul(2))
            .saturating_add(DATAFUSION_STREAMING_DECODED_WINDOW_BYTES)
            .saturating_add(inputs.batch_allocation_bytes)
    } else if requires_sort {
        datafusion_peak_with_headroom
    } else {
        inputs
            .retained_operator_bytes
            .saturating_add(inputs.batch_allocation_bytes)
    };
    let estimated_datafusion_peak_bytes =
        datafusion_peak_with_headroom.max(datafusion_progress_peak_bytes);

    let peak_bytes = if large_sorted {
        fixed_heap_peak_bytes.max(
            inputs
                .retained_operator_bytes
                .saturating_add(estimated_datafusion_peak_bytes / 2)
                .saturating_add(SORT_TEMPORARY_HEADROOM_BYTES),
        )
    } else if requires_sort {
        fixed_heap_peak_bytes.max(estimated_datafusion_peak_bytes.saturating_mul(3) / 4)
    } else {
        fixed_heap_peak_bytes
    }
    .saturating_add(HEAP_FIXED_HEADROOM_BYTES);

    peak_bytes.saturating_add(peak_bytes / 50)
}

/// Row counts and decoded sizes the rest of the estimate is scaled by.
struct DecodedGeometry {
    /// Rows across every input data file that reported a count.
    record_count: usize,
    /// Per-row bytes used to size one record batch.
    batch_row_width: usize,
    /// Decoded bytes the inputs expand to.
    decoded_input_bytes: usize,
    /// Decoded bytes the outputs expand to.
    decoded_output_bytes: usize,
}

/// Derives decoded geometry from the manifest, falling back when counts are absent.
///
/// The exact path multiplies a fixed row width by the counted rows. When the
/// schema has no fixed width, or any file omitted its count, the fallback
/// inflates compressed bytes instead — and keeps whole-input inflation separate
/// from batch sizing, so missing counts cannot turn the entire compressed input
/// into one synthetic row-sized batch.
fn decoded_geometry(
    data_files: &[FileScanTask],
    schema_row_width: Option<usize>,
    hidden_row_width: usize,
    compressed_input_bytes: usize,
) -> DecodedGeometry {
    let mut record_count = 0usize;
    let mut has_complete_record_counts = true;
    for task in data_files {
        match task.record_count {
            Some(count) => {
                record_count = record_count.saturating_add(widen(count));
            }
            None => has_complete_record_counts = false,
        }
    }
    if let (Some(schema_row_width), true) = (schema_row_width, has_complete_record_counts) {
        let row_width = schema_row_width.saturating_add(hidden_row_width);
        return DecodedGeometry {
            record_count,
            batch_row_width: row_width,
            decoded_input_bytes: row_width.saturating_mul(record_count),
            decoded_output_bytes: schema_row_width.saturating_mul(record_count),
        };
    }
    let fallback_row_count = record_count.max(1);
    let fallback_row_width = compressed_input_bytes
        .checked_div(fallback_row_count)
        .unwrap_or_default()
        .saturating_mul(COMPRESSED_TO_DECODED_INFLATION)
        .max(1);
    let decoded_output_bytes =
        compressed_input_bytes.saturating_mul(COMPRESSED_TO_DECODED_INFLATION);
    DecodedGeometry {
        record_count,
        batch_row_width: fallback_row_width.saturating_add(hidden_row_width),
        decoded_input_bytes: decoded_output_bytes
            .saturating_add(hidden_row_width.saturating_mul(record_count)),
        decoded_output_bytes,
    }
}

/// Estimates the peak heap bytes of one compaction plan for scheduler admission.
pub(crate) fn estimate_plan_memory(
    plan: &CompactionPlan,
    schema: &Schema,
    format_version: FormatVersion,
    max_record_batch_rows: usize,
    enable_prefetch: bool,
    requires_sort: bool,
) -> usize {
    let data_files = &plan.file_group.data_files;
    let compressed_input_bytes = sum_file_sizes(data_files.iter());
    let prefetch = if enable_prefetch {
        // `iceberg-compaction-core` stages one complete file per active scan partition in a
        // memory-backed `FileIO`. The decoded batches are accounted separately below, so the
        // staged compressed bytes must not be multiplied by another decoded-copy factor.
        estimate_prefetch_bytes(plan, format_version)
    } else {
        0
    };

    let schema_row_width = estimated_schema_row_width(schema);
    let hidden_row_width = hidden_row_width(plan, format_version);
    let DecodedGeometry {
        record_count,
        batch_row_width,
        decoded_input_bytes,
        decoded_output_bytes,
    } = decoded_geometry(
        data_files,
        schema_row_width,
        hidden_row_width,
        compressed_input_bytes,
    );

    // Equality deletes are broadcast and fully materialized by the merge-on-read plan.
    let equality_delete_bytes = estimate_equality_delete_join_bytes(plan);

    // Pre-V3 position deletes build a full anti-join hash table. V3 deletion vectors do not.
    let position_delete_raw_bytes = if format_version < FormatVersion::V3 {
        sum_file_sizes(plan.file_group.position_delete_files.iter())
    } else {
        0
    };

    let position_delete_join_bytes = if position_delete_raw_bytes > 0 {
        let compressed_estimate = position_delete_raw_bytes
            .saturating_mul(POSITION_DELETE_INFLATION)
            .saturating_add(estimate_position_delete_join_overhead_bytes(plan));
        // HashJoin retains both the collected build-side batches and its hash table for the whole
        // probe stream, so both fully overlap a SortExec consuming and buffering the join output.
        compressed_input_bytes.saturating_add(compressed_estimate)
    } else {
        0
    };

    let executor_parallelism = plan.recommended_executor_parallelism().max(1);
    let output_parallelism = plan.recommended_output_parallelism().max(1);
    let sort_workspace_bytes = if requires_sort {
        // SortExec preserves and concurrently executes every output partition, so its tracked
        // workspace covers the full decoded input rather than a single partition.
        decoded_output_bytes
    } else {
        0
    };
    // Every sorted output partition pins a merge reservation before reading data.
    let sort_merge_headroom_bytes = if requires_sort {
        DATAFUSION_SORT_MERGE_RESERVATION_BYTES.saturating_mul(output_parallelism)
    } else {
        0
    };
    let batch_allocation_bytes = batch_row_width.saturating_mul(max_record_batch_rows);
    let concurrent_batch_allocation_bytes =
        batch_allocation_bytes.saturating_mul(executor_parallelism);
    // Every scan partition has its own buffer. Streaming plans keep two batches in flight per
    // partition; sorted plans keep one before handing it to the sorter.
    let batch_overhead_bytes = if requires_sort {
        concurrent_batch_allocation_bytes
    } else {
        concurrent_batch_allocation_bytes.saturating_mul(2)
    };
    let (retained_operator_bytes, estimated_datafusion_peak_bytes) = datafusion_pool_bytes(
        requires_sort,
        &PoolInputs {
            data_file_count: data_files.len(),
            executor_parallelism,
            compressed_input_bytes,
            decoded_input_bytes,
            prefetch,
            equality_delete_bytes,
            position_delete_join_bytes,
            sort_workspace_bytes,
            sort_merge_headroom_bytes,
            batch_allocation_bytes,
            batch_overhead_bytes,
        },
    );

    heap_peak_bytes(
        plan,
        format_version,
        requires_sort,
        &HeapInputs {
            data_file_count: data_files.len(),
            executor_parallelism,
            output_parallelism,
            record_count,
            max_record_batch_rows,
            hidden_row_width,
            schema_row_width,
            decoded_input_bytes,
            decoded_output_bytes,
            equality_delete_bytes,
            position_delete_raw_bytes,
            batch_allocation_bytes,
            batch_overhead_bytes,
            retained_operator_bytes,
            estimated_datafusion_peak_bytes,
        },
    )
}

fn estimate_prefetch_bytes(plan: &CompactionPlan, format_version: FormatVersion) -> usize {
    let concurrency = plan.recommended_executor_parallelism().max(1);
    let data = estimate_provider_prefetch(
        plan.file_group.data_files.iter().filter(|task| {
            format_version < FormatVersion::V3
                || !task
                    .deletes
                    .iter()
                    .any(|delete| delete.file_type == DataContentType::PositionDeletes)
        }),
        concurrency,
    );
    // Delete files that get their own table provider are scanned -- and therefore prefetched --
    // concurrently with the data scan, because `DatafusionTableRegister` passes the same
    // `enable_prefetch` flag to every provider it builds. From V3 on, position deletes are
    // deletion vectors attached to the data task instead of a registered table, so only the
    // pre-V3 position-delete provider counts here.
    let position_deletes = if format_version < FormatVersion::V3 {
        estimate_provider_prefetch(plan.file_group.position_delete_files.iter(), concurrency)
    } else {
        0
    };
    let equality_deletes =
        estimate_provider_prefetch(plan.file_group.equality_delete_files.iter(), concurrency);

    data.saturating_add(position_deletes)
        .saturating_add(equality_deletes)
}

fn estimate_provider_prefetch<'a>(
    tasks: impl Iterator<Item = &'a FileScanTask>,
    concurrency: usize,
) -> usize {
    let mut file_sizes = tasks
        .map(|task| widen(task.file_size_in_bytes))
        .collect::<Vec<_>>();
    file_sizes.sort_unstable_by(|left, right| right.cmp(left));
    file_sizes
        .into_iter()
        .take(concurrency)
        .fold(0usize, usize::saturating_add)
}

fn sum_file_sizes<'a>(tasks: impl Iterator<Item = &'a FileScanTask>) -> usize {
    tasks
        .map(|task| widen(task.file_size_in_bytes))
        .fold(0usize, usize::saturating_add)
}

fn estimate_position_delete_join_overhead_bytes(plan: &CompactionPlan) -> usize {
    let Some(record_count) = complete_record_count(&plan.file_group.position_delete_files) else {
        return 0;
    };

    HASH_JOIN_ROW_OVERHEAD_BYTES
        .saturating_add(POSITION_DELETE_KEY_OVERHEAD_BYTES)
        .saturating_mul(record_count)
}

fn estimate_equality_delete_join_bytes(plan: &CompactionPlan) -> usize {
    let compressed_estimate = sum_file_sizes(plan.file_group.equality_delete_files.iter())
        .saturating_mul(EQUALITY_DELETE_INFLATION);
    let row_overhead = complete_record_count(&plan.file_group.equality_delete_files)
        .unwrap_or_default()
        .saturating_mul(HASH_JOIN_ROW_OVERHEAD_BYTES);

    compressed_estimate.saturating_add(row_overhead)
}

fn complete_record_count(tasks: &[FileScanTask]) -> Option<usize> {
    tasks.iter().try_fold(0usize, |total, task| {
        let count = usize::try_from(task.record_count?).unwrap_or(usize::MAX);
        Some(total.saturating_add(count))
    })
}

fn position_delete_row_width(plan: &CompactionPlan) -> usize {
    let max_data_path_width = plan
        .file_group
        .data_files
        .iter()
        .map(|task| task.data_file_path.len())
        .max()
        .unwrap_or_default();
    max_data_path_width
        .saturating_add(size_of::<i32>())
        .saturating_add(size_of::<i64>())
}

fn hidden_row_width(plan: &CompactionPlan, format_version: FormatVersion) -> usize {
    // Match DataFusionTaskContextBuilder: equality deletes add an i64 sequence number, while
    // pre-V3 position deletes add the data path Utf8 array and an i64 row position.
    let sequence_number_width = if plan.file_group.equality_delete_files.is_empty() {
        0
    } else {
        size_of::<i64>()
    };
    let position_delete_width = if format_version < FormatVersion::V3
        && !plan.file_group.position_delete_files.is_empty()
    {
        position_delete_row_width(plan)
    } else {
        0
    };

    sequence_number_width.saturating_add(position_delete_width)
}

fn estimated_schema_row_width(schema: &Schema) -> Option<usize> {
    let mut width = 0usize;
    for field in schema.as_struct().fields() {
        let Type::Primitive(primitive) = field.field_type.as_ref() else {
            return None;
        };
        let field_width = primitive_width(primitive)?;
        width = width.saturating_add(field_width);
    }
    (width > 0).then_some(width)
}

fn primitive_width(primitive: &PrimitiveType) -> Option<usize> {
    let width = match primitive {
        PrimitiveType::Boolean => 1,
        PrimitiveType::Int | PrimitiveType::Float | PrimitiveType::Date => 4,
        PrimitiveType::Long
        | PrimitiveType::Double
        | PrimitiveType::Time
        | PrimitiveType::Timestamp
        | PrimitiveType::Timestamptz
        | PrimitiveType::TimestampNs
        | PrimitiveType::TimestamptzNs => 8,
        PrimitiveType::Decimal { .. } | PrimitiveType::Uuid => 16,
        PrimitiveType::Fixed(size) => widen(*size),
        PrimitiveType::String | PrimitiveType::Binary => return None,
    };
    Some(width)
}

#[cfg(test)]
/// Branch coverage for the ported estimator.
mod tests {
    use std::sync::Arc;

    use iceberg::spec::{DataFileFormat, FormatVersion, NestedField, PrimitiveType, Schema, Type};
    use iceberg_compaction_core::compaction::CompactionPlan;
    use iceberg_compaction_core::file_selection::FileGroup;

    use super::{FileScanTask, estimate_plan_memory};

    /// Builds a one-column schema of the given primitive type.
    fn schema_of(primitive: PrimitiveType) -> Schema {
        Schema::builder()
            .with_schema_id(0)
            .with_fields(vec![
                NestedField::required(1, "value", Type::Primitive(primitive)).into(),
            ])
            .build()
            .expect("a one-column schema builds")
    }

    /// Builds one scan task with an explicit compressed size and row count.
    fn task(path: &str, bytes: u64, rows: Option<u64>, schema: &Schema) -> FileScanTask {
        FileScanTask::builder()
            .with_file_size_in_bytes(bytes)
            .with_start(0)
            .with_length(bytes)
            .with_record_count(rows)
            .with_data_file_path(path.to_string())
            .with_data_file_format(DataFileFormat::Parquet)
            .with_schema(Arc::new(schema.clone()))
            .with_project_field_ids(vec![1])
            .with_case_sensitive(true)
            .build()
    }

    /// Assembles a plan from explicit data and delete groups.
    fn plan_of(
        data_files: Vec<FileScanTask>,
        position_delete_files: Vec<FileScanTask>,
        equality_delete_files: Vec<FileScanTask>,
        executor_parallelism: usize,
        output_parallelism: usize,
    ) -> CompactionPlan {
        let total_size = data_files.iter().map(|task| task.length).sum();
        let data_file_count = data_files.len();
        CompactionPlan::new(
            FileGroup {
                data_files,
                position_delete_files,
                equality_delete_files,
                total_size,
                data_file_count,
                executor_parallelism,
                output_parallelism,
            },
            "main",
            1,
        )
    }

    /// A stock production plan is admissible on a documented 8 GiB Forge pod.
    ///
    /// Two standard 512 MiB variable-width inputs, the production batch width,
    /// prefetch, and a sort are the heaviest shape the ordinary compaction
    /// route plans. The 80-percent default budget of a dedicated 8 GiB pod is
    /// 6.4 GiB, and this pins that such a plan is admitted there rather than
    /// requiring the oversized pod an earlier qualification journey injected.
    ///
    /// The estimate is admission control, not an allocation guarantee: it
    /// decides whether the plan may start, and the execution engine still
    /// spills and bounds its own pools at run time.
    ///
    /// # Panics
    ///
    /// Panics when the stock plan no longer fits the documented pod budget.
    #[test]
    fn stock_variable_width_plan_fits_documented_production_pod() {
        const STOCK_INPUT_BYTES: u64 = 512 * 1024 * 1024;
        const STOCK_ROW_BYTES: u64 = 1024;
        /// Four fifths of a dedicated 8 GiB Forge pod.
        const POD_BUDGET_BYTES: usize = 8 * 1024 * 1024 * 1024 / 5 * 4;

        let variable = schema_of(PrimitiveType::String);
        let rows = STOCK_INPUT_BYTES / STOCK_ROW_BYTES;
        let plan = plan_of(
            vec![
                task("stock-0.parquet", STOCK_INPUT_BYTES, Some(rows), &variable),
                task("stock-1.parquet", STOCK_INPUT_BYTES, Some(rows), &variable),
            ],
            Vec::new(),
            Vec::new(),
            4,
            4,
        );

        let estimate = estimate_plan_memory(&plan, &variable, FormatVersion::V2, 1024, true, true);
        assert!(
            estimate <= POD_BUDGET_BYTES,
            "a stock two-input production plan must be admissible on the documented \
             8 GiB pod: estimate {estimate} exceeds the {POD_BUDGET_BYTES} byte budget"
        );
    }

    /// The port reproduces `origin/main`'s estimate on every branch it owns.
    ///
    /// Two fully hand-evaluated anchors pin the streaming and sorted arithmetic
    /// to exact bytes, so any edit to a constant or an operator moves a number
    /// this test names. The remaining cases pin the direction of each branch the
    /// anchors cannot reach — schema fallback, prefetch overlap, both delete
    /// shapes, the V3 deletion-vector exemption, parallelism, and saturation —
    /// because each of those is a distinct upstream code path rather than a
    /// scaling of the same one.
    ///
redacted
    /// `src/storage/src/hummock/compactor/iceberg_compaction/memory.rs`.
    ///
    /// # Panics
    ///
    /// Panics when any anchor moves or when a branch stops changing the estimate
    /// in the direction upstream's arithmetic requires.
    #[test]
redacted
        let fixed = schema_of(PrimitiveType::Long);
        let variable = schema_of(PrimitiveType::String);
        let one_file = || vec![task("data-0.parquet", 10_000, Some(1_000), &fixed)];

        // Anchor 1 — streaming, fixed-width schema, no deletes, no prefetch.
        let streaming = plan_of(one_file(), Vec::new(), Vec::new(), 1, 1);
        let streaming_bytes =
            estimate_plan_memory(&streaming, &fixed, FormatVersion::V2, 1024, false, false);
        assert_eq!(
            streaming_bytes, 310_275,
            "the streaming estimate is fixed by the ported constants"
        );

        // Anchor 2 — the same plan sorted, which pins the merge reservation,
        // the sorted decoded retention, and the sorted headroom together.
        let sorted_bytes =
            estimate_plan_memory(&streaming, &fixed, FormatVersion::V2, 1024, false, true);
        assert_eq!(
            sorted_bytes, 21_136_097,
            "the sorted estimate is fixed by the ported constants"
        );

        // A variable-width schema has no row width, so the estimator falls back
        // to inflating compressed bytes and reaches the other writer branch.
        let variable_bytes =
            estimate_plan_memory(&streaming, &variable, FormatVersion::V2, 1024, false, false);
        assert_ne!(
            variable_bytes, streaming_bytes,
            "a schema with no computable row width takes the fallback path"
        );

        // Prefetch stages the largest files per scan partition. It reaches the
        // result only through the retained-operator term, which the large-sorted
        // branch is the one that reads, so the fixture has to be a real
        // large sort rather than the anchors above.
        let large_sort = plan_of(
            (0..4)
                .map(|index| {
                    task(
                        &format!("bulk-{index}.parquet"),
                        16 * 1024 * 1024,
                        Some(3_000_000),
                        &fixed,
                    )
                })
                .collect(),
            Vec::new(),
            Vec::new(),
            4,
            4,
        );
        assert!(
            estimate_plan_memory(&large_sort, &fixed, FormatVersion::V2, 1024, true, true)
                > estimate_plan_memory(&large_sort, &fixed, FormatVersion::V2, 1024, false, true),
            "staged compressed input adds to a large sort's retained operators"
        );
    }

    /// Delete files, format version, and parallelism move the estimate exactly
    /// as upstream's arithmetic requires.
    ///
    /// Equality deletes are broadcast and fully materialised; a pre-V3 position
    /// delete builds an anti-join hash table that a V3 deletion vector does
    /// not; parallelism multiplies the concurrently allocated batches; and
    /// every accumulation saturates, so an absurd manifest yields a refusably
    /// large figure rather than a wrapped small one or a panic.
    ///
    /// # Panics
    ///
    /// Panics when any branch stops changing the estimate in the direction
    /// upstream's arithmetic requires.
    #[test]
redacted
        let fixed = schema_of(PrimitiveType::Long);
        let one_file = || vec![task("data-0.parquet", 10_000, Some(1_000), &fixed)];
        let streaming = plan_of(one_file(), Vec::new(), Vec::new(), 1, 1);
        let streaming_bytes =
            estimate_plan_memory(&streaming, &fixed, FormatVersion::V2, 1024, false, false);

        // Equality deletes are broadcast and fully materialised.
        let with_equality = plan_of(
            one_file(),
            Vec::new(),
            vec![task("eq-0.parquet", 4_000, Some(100), &fixed)],
            1,
            1,
        );
        assert!(
            estimate_plan_memory(
                &with_equality,
                &fixed,
                FormatVersion::V2,
                1024,
                false,
                false
            ) > streaming_bytes,
            "an equality-delete join adds its build side to the estimate"
        );

        // Pre-V3 position deletes build an anti-join hash table; a V3 deletion
        // vector attaches to the data task and builds none, so the identical
        // plan must cost less under V3.
        let with_position = plan_of(
            one_file(),
            vec![task("pos-0.parquet", 4_000, Some(100), &fixed)],
            Vec::new(),
            1,
            1,
        );
        let v2_position = estimate_plan_memory(
            &with_position,
            &fixed,
            FormatVersion::V2,
            1024,
            false,
            false,
        );
        let v3_position = estimate_plan_memory(
            &with_position,
            &fixed,
            FormatVersion::V3,
            1024,
            false,
            false,
        );
        assert!(
            v2_position > streaming_bytes,
            "a pre-V3 position-delete join adds its hash table to the estimate"
        );
        assert!(
            v3_position < v2_position,
            "a V3 deletion vector builds no join, so it must not be charged for one"
        );
    }

    /// Parallelism multiplies concurrently allocated batches, and every
    /// accumulation saturates.
    ///
    /// An absurd manifest must yield a refusably large figure rather than a
    /// wrapped small one the queue would admit, or a panic.
    ///
    /// # Panics
    ///
    /// Panics when parallelism stops raising the estimate or when saturation
    /// does not hold.
    #[test]
redacted
        let fixed = schema_of(PrimitiveType::Long);

        // Parallelism multiplies the concurrently allocated batches.
        let wide = plan_of(
            (0..8)
                .map(|index| {
                    task(
                        &format!("data-{index}.parquet"),
                        10_000,
                        Some(1_000),
                        &fixed,
                    )
                })
                .collect(),
            Vec::new(),
            Vec::new(),
            8,
            4,
        );
        let narrow = plan_of(
            (0..8)
                .map(|index| {
                    task(
                        &format!("data-{index}.parquet"),
                        10_000,
                        Some(1_000),
                        &fixed,
                    )
                })
                .collect(),
            Vec::new(),
            Vec::new(),
            1,
            1,
        );
        assert!(
            estimate_plan_memory(&wide, &fixed, FormatVersion::V2, 1024, true, false)
                > estimate_plan_memory(&narrow, &fixed, FormatVersion::V2, 1024, true, false),
            "more scan partitions hold more batches and prefetched files at once"
        );

        // Every accumulation saturates, so an absurd manifest yields a refusably
        // large number rather than a wrapped small one or a panic.
        let saturating = plan_of(
            vec![task("huge.parquet", u64::MAX, Some(u64::MAX), &fixed)],
            Vec::new(),
            Vec::new(),
            usize::MAX,
            usize::MAX,
        );
        let saturated = estimate_plan_memory(
            &saturating,
            &fixed,
            FormatVersion::V2,
            usize::MAX,
            true,
            true,
        );
        assert!(
            saturated > usize::MAX / 2,
            "saturating arithmetic keeps an absurd plan absurdly large"
        );
    }
}
