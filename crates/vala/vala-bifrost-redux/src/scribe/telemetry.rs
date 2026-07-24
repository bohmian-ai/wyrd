//! Optional stage measurements for real Scribe workload runs.

use crate::scribe::admission::AdmissionSnapshot;

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
    /// Post-ACK preprocessing and reconstruction lane.
    pub persistence: ExecutorSnapshot,
    /// WAL filesystem lane.
    pub wal_io: ExecutorSnapshot,
    /// Fixed shard owner topology and health metrics.
    pub shards: ShardHealthSnapshot,
    /// Accepted and fsynced frame counters used to expose the durability gap.
    pub durability: ScribeDurabilitySnapshot,
}

/// Monotonic frame counters for the accepted-to-fsynced durability window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScribeDurabilitySnapshot {
    /// Frames acknowledged after bounded writer-queue admission.
    pub accepted_frames: u64,
    /// Rows acknowledged after bounded writer-queue admission.
    pub accepted_rows: u64,
    /// Frames whose WAL data reached `sync_data`.
    pub fsynced_frames: u64,
    /// Rows whose WAL data reached `sync_data`.
    pub fsynced_rows: u64,
}

impl ScribeDurabilitySnapshot {
    /// Return the current acknowledged-but-not-fsynced frame count.
    #[must_use]
    pub const fn frame_gap(self) -> u64 {
        self.accepted_frames.saturating_sub(self.fsynced_frames)
    }
}
