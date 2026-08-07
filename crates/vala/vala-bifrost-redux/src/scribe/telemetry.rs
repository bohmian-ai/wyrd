//! Optional stage measurements for real Scribe workload runs.

use crate::scribe::admission::AdmissionSnapshot;
use crate::scribe::memory::MEMORY_CATEGORY_COUNT;
use crate::scribe::seal_key::SealKey;

/// Memory ownership for one bounded tenant/table/day bucket in inspection data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScribeBucketMemorySnapshot {
    /// Exact bucket identity. This is an inspection payload, not a metric label.
    pub seal_key: SealKey,
    /// Writable Arrow bytes.
    pub writable_bytes: usize,
    /// Immutable Arrow bytes.
    pub immutable_bytes: usize,
}

/// Complete bounded setup snapshot used by Bifrost test harnesses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScribeInspectionSnapshot {
    /// Number of fixed shard owner tasks.
    pub shard_task_count: usize,
    /// Number of fixed shard command channels.
    pub shard_channel_count: usize,
    /// Number of currently open shard WAL streams.
    pub open_wal_stream_count: usize,
    /// Accepted append commands waiting in shard scheduling.
    pub queued_items: usize,
    /// Writable bucket count.
    pub writable_bucket_count: usize,
    /// Immutable bucket count.
    pub immutable_bucket_count: usize,
    /// Category totals in [`MemoryCategory`](crate::scribe::memory::MemoryCategory) order.
    pub memory_by_category: [usize; MEMORY_CATEGORY_COUNT],
    /// Memory totals distributed across the fixed shard owners.
    pub memory_by_shard: [usize; crate::scribe::routing::SCRIBE_SHARD_COUNT],
    /// Exact bucket-level memory ownership.
    pub memory_by_bucket: Vec<ScribeBucketMemorySnapshot>,
    /// Sum of all governor category totals.
    pub total_accounted_memory: usize,
    /// Parent Bifrost bytes currently reserved across all roles.
    pub parent_used_memory: usize,
    /// Parent Bifrost memory ceiling.
    pub parent_memory_limit: usize,
    /// Scribe child bytes currently reserved.
    pub scribe_used_memory: usize,
    /// Scribe child memory ceiling.
    pub scribe_memory_limit: usize,
    /// Current ingress bytes charged against the ingress ceiling.
    ///
    /// Equals `scribe_used_memory`; ingress admission charges the whole Scribe
    /// child total against the (smaller) ingress ceiling. Exposed so harnesses
    /// can observe the D83 pressure-seal watermark decision — occupancy against
    /// [`Self::ingress_memory_limit`] and the high/low-water marks below —
    /// without recomputing the runtime pressure config.
    pub ingress_used_memory: usize,
    /// Ingress reservation ceiling (`limit_bytes - persistence_headroom`).
    pub ingress_memory_limit: usize,
    /// High-water byte threshold that triggers a coordinated pressure seal.
    pub ingress_high_water_memory: usize,
    /// Low-water byte target that pressure sealing drains toward.
    pub ingress_low_water_memory: usize,
    /// WAL bytes retained on disk.
    pub wal_disk_bytes: u64,
}

/// Point-in-time health and queue metrics for the fixed shard owners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShardHealthSnapshot {
    /// Fixed shard owner tasks.
    pub shard_tasks: usize,
    /// Fixed bounded shard command channels.
    pub shard_channels: usize,
    /// Items currently accepted by the shard runtime.
    pub pending_items: usize,
    /// Terminal shard owner errors.
    pub terminal_errors: usize,
}

/// Aggregate queue metrics for runtime dashboards.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExecutorSnapshot {
    /// Operations currently queued or executing on this lane.
    pub depth: usize,
    /// Application queue capacity for this lane.
    pub capacity: usize,
    /// Number of submissions that encountered lane saturation.
    pub saturation_events: u64,
    /// Work items completed successfully.
    pub completed: u64,
    /// Work items that returned an error.
    pub failed: u64,
    /// Work items terminated by a worker panic.
    pub panicked: u64,
}

/// Point-in-time queue, admission, execution-lane, and shard health telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScribeRuntimeSnapshot {
    /// Pod-global admission counters and limits.
    pub admission: AdmissionSnapshot,
    /// Aggregate execution-lane queue metrics.
    pub executor: ExecutorSnapshot,
    /// Pre-ACK decode and projection lane.
    pub ingress: ExecutorSnapshot,
    /// Pre-ACK preprocessing and reconstruction lane.
    pub persistence: ExecutorSnapshot,
    /// WAL filesystem lane.
    pub wal_io: ExecutorSnapshot,
    /// Fixed shard owner topology and health metrics.
    pub shards: ShardHealthSnapshot,
}
