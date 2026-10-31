//! Per-request Scribe write profile: where one write's time to its durable
//! acknowledgement goes.
//!
//! One [`WriteProfile`] follows one admitted write request from transport
//! admission to its acknowledgement and is emitted once, when its last owner
//! drops, as [`ProfileFamily::SCRIBE_WRITE`]. Its steps are `DataFusion`
//! [`Time`] metrics timed with the same scoped guards the Oracle's query
//! profile uses. A shard commits requests in groups and every request in a
//! group waits for the whole serial cycle, so the shard times each
//! [`GroupStages`] step once and charges it to every request of the group.

use std::collections::BTreeMap;
use std::time::Duration;

use datafusion::physical_plan::metrics::Time;

use crate::profile::{ProfileFamily, ProfileValue};

/// Prefix naming Scribe's steps among the emitted events.
const SCRIBE_EVENT: &str = "Scribe";

/// The benchmark timing boundary of Scribe's part of one acknowledged write:
/// transport admission until the request task resumes with its durable
/// acknowledgement. Each [`WriteProfile::record_timing`] part is its own
/// boundary under `bifrost.scribe.write.`.
pub(crate) const WRITE_TIMING: &str = "bifrost.scribe.write";

/// The serial group-commit stages one shard cycle runs, in order.
#[derive(Debug, Default)]
pub(crate) struct GroupStages {
    /// Writing the group's SLICE records to the WAL.
    pub(crate) write: Time,
    /// Appending each complete batch's COMMIT record.
    pub(crate) commit: Time,
    /// The one sync making the group's SLICE and COMMIT records durable.
    pub(crate) sync: Time,
    /// Committing the batch-control fences in `PostgreSQL`.
    pub(crate) fence: Time,
    /// Inserting the committed rows into the memtable, making them visible.
    pub(crate) insert: Time,
}

impl GroupStages {
    /// Adds every stage of `other` into this request's stages.
    pub(crate) fn add(&self, other: &Self) {
        for ((_, mine), (_, theirs)) in self.steps().into_iter().zip(other.steps()) {
            mine.add(theirs);
        }
    }

    /// Every stage with its event name, in cycle order.
    const fn steps(&self) -> [(&'static str, &Time); 5] {
        [
            ("group_write", &self.write),
            ("group_commit", &self.commit),
            ("group_sync", &self.sync),
            ("group_fence", &self.fence),
            ("group_insert", &self.insert),
        ]
    }
}

/// Accumulated step times for one write request, emitted when it drops.
///
/// The request's own steps run before its shard dispatch; its group stages
/// are charged when its group acknowledges it. A request refused before then
/// emits the steps it reached.
#[derive(Debug, Default)]
pub(crate) struct WriteProfile {
    /// Transport admission: tenant, table, schema, and memory admission.
    pub(crate) admission: Time,
    /// Decoding and stamping the request's rows, ingress-lane queueing
    /// included.
    pub(crate) decode: Time,
    /// Splitting and serializing the rows into WAL slices, ingress-lane
    /// queueing included.
    pub(crate) preprocess: Time,
    /// From preprocessing's end until the request's group starts: the memory
    /// charge, the shard mailbox, and earlier groups.
    pub(crate) shard_wait: Time,
    /// The group-commit cycle the request was acknowledged by.
    pub(crate) group: GroupStages,
}

impl WriteProfile {
    /// Records this acknowledged write through
    /// [`wyrd_telemetry::record_timing_parts`] at [`WRITE_TIMING`] under the
    /// current span's benchmark classification: `elapsed`, then each step as
    /// a part.
    ///
    /// The parts are `admission` (tenant, table, schema, and memory
    /// validation), `decode`, `preprocess`, `shard_wait`, `wal_append` (the
    /// group's SLICE and COMMIT records), `wal_sync`, `visibility` (fences
    /// and memtable insert), and `ack`, the delivery from the shard's send
    /// until the request task resumed. Group stages are shared: every
    /// request of a group carries the whole stage, so parts overlap across
    /// requests and are never summed into a latency.
    pub(crate) fn record_timing(&self, elapsed: Duration, ack: Duration) {
        let time =
            |time: &Time| Duration::from_nanos(u64::try_from(time.value()).unwrap_or(u64::MAX));
        let group = &self.group;
        wyrd_telemetry::record_timing_parts(
            &tracing::Span::current(),
            WRITE_TIMING,
            "success",
            elapsed,
            None,
            &[
                ("bifrost.scribe.write.admission", time(&self.admission)),
                ("bifrost.scribe.write.decode", time(&self.decode)),
                ("bifrost.scribe.write.preprocess", time(&self.preprocess)),
                ("bifrost.scribe.write.shard_wait", time(&self.shard_wait)),
                (
                    "bifrost.scribe.write.wal_append",
                    time(&group.write) + time(&group.commit),
                ),
                ("bifrost.scribe.write.wal_sync", time(&group.sync)),
                (
                    "bifrost.scribe.write.visibility",
                    time(&group.fence) + time(&group.insert),
                ),
                ("bifrost.scribe.write.ack", ack),
            ],
        );
    }

    /// Every step as a `Scribe.<step>` nanosecond event.
    fn events(&self) -> BTreeMap<String, ProfileValue> {
        [
            ("admission", &self.admission),
            ("decode", &self.decode),
            ("preprocess", &self.preprocess),
            ("shard_wait", &self.shard_wait),
        ]
        .into_iter()
        .chain(self.group.steps())
        .map(|(step, time)| {
            (
                format!("{SCRIBE_EVENT}.{step}"),
                ProfileValue::Nanoseconds(u64::try_from(time.value()).unwrap_or(u64::MAX)),
            )
        })
        .collect()
    }
}

impl Drop for WriteProfile {
    /// Emits the profile once as [`ProfileFamily::SCRIBE_WRITE`]: global
    /// counters, divided by the profile count for the mean per request, and
    /// one DEBUG event.
    fn drop(&mut self) {
        ProfileFamily::SCRIBE_WRITE.emit(&self.events());
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{GroupStages, WriteProfile};
    use crate::profile::ProfileValue;

    /// A request's own steps and the group stages charged to it both appear
    /// as `Scribe.<step>` events, and charging two groups sums them.
    ///
    /// # Panics
    ///
    /// Panics when an event is missing or holds the wrong time.
    #[test]
    fn profile_carries_request_steps_and_charged_group_stages() {
        let profile = WriteProfile::default();
        profile.decode.add_duration(Duration::from_micros(3));
        let stages = GroupStages::default();
        stages.fence.add_duration(Duration::from_micros(5));
        profile.group.add(&stages);
        profile.group.add(&stages);

        let events = profile.events();

        assert_eq!(
            events.get("Scribe.decode"),
            Some(&ProfileValue::Nanoseconds(3_000))
        );
        assert_eq!(
            events.get("Scribe.group_fence"),
            Some(&ProfileValue::Nanoseconds(10_000))
        );
    }
}
