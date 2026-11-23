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

/// Benchmark-only histogram of one shard group's serial work, by `stage`,
/// each recorded once per committed group:
///
/// - `cycle`: the whole group, from the shard taking it until its last ACK;
///   every other stage nests inside it.
/// - `rotate`: the retained-commit retry and the generation rotation check
///   before the group writes.
/// - `prepare`: duplicate and identity checks, memory accounting, and WAL
///   record construction: the group's WAL slice writing less `convert` and
///   `wal_write`.
/// - `convert`: elapsed Variant layout conversion of the group's slices on the
///   ingress CPU lane, up to two at once, lane queueing included.
/// - `convert_job`: one slice's conversion on its lane worker, recorded per
///   slice; summed over a group it is the conversion CPU time the `convert`
///   elapsed time overlaps.
/// - `wal_write`: the group's SLICE records on the WAL IO lane, lane queueing
///   included.
/// - `commit`, `sync`, `fence`, `insert`: the [`GroupStages`] steps; `commit`
///   and `sync` include WAL IO lane queueing.
/// - `fence_conn`: inside `fence`, waiting for the tenant's `PostgreSQL`
///   connections; the rest of `fence` is its statements and commit.
/// - `publish`: recording the committed batches and publishing the
///   inspection snapshot, after `insert` and before any ACK.
/// - `ack`: completing every waiter of the group.
pub(crate) const GROUP_STAGE_SECONDS: &str = "bifrost_scribe_group_stage_seconds";

/// Benchmark-only histogram of one shard group's size, by `measure`, each
/// recorded once per group: `requests`, `slices`, `rows`, `logical_bytes`
/// (the slices' logical Arrow bytes), and `wal_bytes` (the SLICE records'
/// encoded payload bytes) once it commits, and when the shard takes it,
/// `queued_requests` and `queued_bytes` still waiting behind it.
pub(crate) const GROUP_SIZE: &str = "bifrost_scribe_group_size";

/// Benchmark-only count of shard groups by why they closed, `reason`:
/// `limit` at [`crate::scribe::shards::MAX_GROUP_ITEMS`] requests,
/// `duplicate` before a second copy of one batch, or `exhausted` when no
/// request was left.
pub(crate) const GROUP_CLOSE_TOTAL: &str = "bifrost_scribe_group_close_total";

/// Benchmark counter of optional Variant conversions that found no ingress
/// lane slot or root capacity and waited for the group's oldest conversion
/// instead of starting.
pub(crate) const CONVERSION_DEFERRED_TOTAL: &str = "bifrost_scribe_conversion_deferred_total";

/// Benchmark-only count of the rows of captured memtable batches live reads
/// examined: each batch its bounds did not prune, counted once when the read
/// evaluates it, before any row filter, so it measures the input a read
/// worked through rather than the rows it returned. A memory batch reads no
/// storage, so these rows are never Oracle's physical `rows_scanned`.
pub(crate) const LIVE_ROWS_EXAMINED_TOTAL: &str = "bifrost_scribe_live_rows_examined_total";

/// Benchmark-only count of the decoded Arrow bytes of the columns live reads
/// examined in the batches [`LIVE_ROWS_EXAMINED_TOTAL`] counts: each read
/// column's recorded byte size, the in-memory bytes the read worked through.
/// It is never Oracle's physical `bytes_scanned`, which counts Parquet bytes
/// read from storage.
pub(crate) const LIVE_BYTES_EXAMINED_TOTAL: &str = "bifrost_scribe_live_bytes_examined_total";

#[cfg(feature = "test-support")]
thread_local! {
    /// Rows and bytes [`record_live_scan`] counted on this thread, mirrored
    /// for tests: per thread, so one test's reads never count another's.
    pub(crate) static LIVE_SCAN: std::cell::Cell<(u64, u64)> =
        const { std::cell::Cell::new((0, 0)) };
}

/// Counts one examined captured batch's `rows` and decoded `bytes` into
/// [`LIVE_ROWS_EXAMINED_TOTAL`] and [`LIVE_BYTES_EXAMINED_TOTAL`], only while
/// benchmarking.
pub(crate) fn record_live_scan(rows: u64, bytes: u64) {
    #[cfg(feature = "test-support")]
    {
        LIVE_SCAN.with(|counted| {
            let (counted_rows, counted_bytes) = counted.get();
            counted.set((counted_rows + rows, counted_bytes + bytes));
        });
    }
    if wyrd_benchmark::enabled() {
        metrics::counter!(LIVE_ROWS_EXAMINED_TOTAL).increment(rows);
        metrics::counter!(LIVE_BYTES_EXAMINED_TOTAL).increment(bytes);
    }
}

/// Records `elapsed` as one observation of group `stage` of
/// [`GROUP_STAGE_SECONDS`], only while benchmarking.
pub(crate) fn record_group_stage(stage: &'static str, elapsed: Duration) {
    if wyrd_benchmark::enabled() {
        metrics::histogram!(GROUP_STAGE_SECONDS, "stage" => stage).record(elapsed.as_secs_f64());
    }
}

/// Records `value` as one observation of group `measure` of [`GROUP_SIZE`],
/// only while benchmarking; a value past `u32::MAX` is recorded as that.
pub(crate) fn record_group_size(measure: &'static str, value: usize) {
    if wyrd_benchmark::enabled() {
        let value = f64::from(u32::try_from(value).unwrap_or(u32::MAX));
        metrics::histogram!(GROUP_SIZE, "measure" => measure).record(value);
    }
}

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

    /// Records each stage once as its [`GROUP_STAGE_SECONDS`] observation,
    /// only while benchmarking.
    pub(crate) fn record_group(&self) {
        for (stage, time) in [
            ("commit", &self.commit),
            ("sync", &self.sync),
            ("fence", &self.fence),
            ("insert", &self.insert),
        ] {
            let nanos = u64::try_from(time.value()).unwrap_or(u64::MAX);
            record_group_stage(stage, Duration::from_nanos(nanos));
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
    /// [`wyrd_benchmark::record_timing_parts`] at [`WRITE_TIMING`] under the
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
        wyrd_benchmark::record_timing_parts(
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
