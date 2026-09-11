//! One exact-partition contract spanning admission accounting, WAL v6 replay,
//! and live-tail identity.
//!
//! These three seams share one failure mode: if two different physical
//! partitions can ever compare, hash, or path-encode as one, the same rows are
//! either read twice or skipped. The proofs live together because no single
//! layer can demonstrate exactness on its own — accounting bounds how many
//! partitions one source may open, WAL replay proves the identity survives a
//! restart, and the durable path projection proves two identities never
//! collide on one key.

use tempfile::TempDir;

use crate::catalog::TableRef;
use crate::catalog::layout::{TimeGranularity, TimePartition};
use crate::namespaces::BifrostNamespace;
use crate::scribe::replay::replay_wal_directory;
use crate::scribe::seal_key::{MAX_TIME_PARTITIONS, SealKey, plan_time_partitions};
use crate::scribe::stream_identity::NodeId;
use crate::scribe::wal::{WalConfig, WalWriter};

/// Builds one Arrow source whose `wyrd_event_time` values are the given
/// epoch-microsecond instants.
fn batch_with_event_times(micros: Vec<i64>) -> arrow::record_batch::RecordBatch {
    use std::sync::Arc;

    use arrow::array::{Int64Array, TimestampMicrosecondArray};
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};

    let rows = micros.len();
    let schema = Arc::new(Schema::new(vec![
        Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        ),
        Field::new("value", DataType::Int64, false),
    ]));
    arrow::record_batch::RecordBatch::try_new(
        schema,
        vec![
            Arc::new(TimestampMicrosecondArray::from(micros)),
            Arc::new(Int64Array::from(
                (0..i64::try_from(rows).expect("fixture row count fits i64")).collect::<Vec<_>>(),
            )),
        ],
    )
    .expect("fixture batch is well formed")
}

/// Admission accounting, WAL v6 replay, and tail identity agree on exactly one
/// partition per row.
///
/// Covers the per-source ceiling at both its accepted and rejected boundary,
/// a round trip of every appended partition through a real WAL directory, and
/// the non-aliasing of `Hour` and `Day` partitions that begin at the same
/// instant.
#[test]
fn time_partition_wal_tail_exact_once() {
    let hour_micros = 3_600_000_000_i64;
    let base = 1_784_016_000_000_000_i64;

    // Exactly the ceiling is admitted from one source.
    let admitted = batch_with_event_times(
        (0..MAX_TIME_PARTITIONS)
            .map(|index| base + hour_micros * i64::try_from(index).expect("index fits i64"))
            .collect(),
    );
    let plan = plan_time_partitions(&admitted, TimeGranularity::Hour)
        .expect("the per-source ceiling is admitted");
    assert_eq!(plan.len(), MAX_TIME_PARTITIONS);

    // One partition past it is refused, and refusal happens during planning,
    // before any WAL work.
    let refused = batch_with_event_times(
        (0..=MAX_TIME_PARTITIONS)
            .map(|index| base + hour_micros * i64::try_from(index).expect("index fits i64"))
            .collect(),
    );
    assert!(
        plan_time_partitions(&refused, TimeGranularity::Hour).is_err(),
        "one partition past the per-source ceiling must be refused"
    );

    // The bound is on partitions, not on days: the same instants that open
    // `MAX_TIME_PARTITIONS` hours open a single day.
    let daily = plan_time_partitions(&admitted, TimeGranularity::Day)
        .expect("the same source is one day at daily granularity");
    assert!(daily.len() < plan.len());

    // Every appended partition survives a WAL v6 round trip with its exact
    // identity, and each is replayed exactly once.
    let temp_dir = TempDir::new().expect("temp WAL directory");
    let node_id = NodeId::new(uuid::Uuid::now_v7());
    let tenant = crate::test_support::tenant();
    let table = TableRef::new(BifrostNamespace::Bifrost, "events");
    let wal = WalWriter::new(
        temp_dir.path(),
        *node_id.as_bytes(),
        1,
        WalConfig::default(),
    )
    .expect("WAL writer");

    let midnight = chrono::DateTime::from_timestamp(1_783_987_200, 0).expect("fixture instant");
    let hour = TimePartition::new(TimeGranularity::Hour, midnight)
        .expect("midnight is an exact hour boundary");
    let next_hour =
        TimePartition::new(TimeGranularity::Hour, midnight + chrono::Duration::hours(1))
            .expect("the following hour is an exact boundary");
    let day = TimePartition::new(TimeGranularity::Day, midnight)
        .expect("midnight is an exact day boundary");

    // `hour` and `day` begin at the same instant, so appending both proves the
    // granularity is part of the durable identity rather than decoration.
    let appended = [hour, next_hour, day];
    for (index, partition) in appended.iter().enumerate() {
        let seal_key = SealKey::new(tenant, table.clone(), *partition);
        let batch_id = [u8::try_from(index).expect("fixture index fits u8"); 16];
        wal.append_and_commit_for_replay_test(
            &seal_key,
            batch_id,
            &[u8::try_from(index).expect("fixture index fits u8")],
        )
        .expect("append and commit");
    }

    let replayed = replay_wal_directory(temp_dir.path()).expect("replay the WAL directory");
    assert_eq!(
        replayed.len(),
        appended.len(),
        "each appended partition must replay to its own distinct cohort"
    );
    for partition in appended {
        let seal_key = SealKey::new(tenant, table.clone(), partition);
        let state = replayed
            .get(&seal_key.as_path_components())
            .unwrap_or_else(|| panic!("partition {partition:?} must replay"));
        assert_eq!(
            state.seal_key, seal_key,
            "replay must recover the exact partition identity, not a truncated one"
        );
        assert_eq!(
            state.data_records.len(),
            1,
            "each committed batch replays exactly once"
        );
    }

    // The two same-instant partitions never collapse onto one durable key, so
    // a tail bound to one of them can neither re-read nor skip the other's
    // rows.
    let hourly_key = SealKey::new(tenant, table.clone(), hour);
    let daily_key = SealKey::new(tenant, table.clone(), day);
    assert_ne!(hourly_key, daily_key);
    assert_ne!(
        hourly_key.as_path_components(),
        daily_key.as_path_components()
    );
    assert_ne!(hour.to_wire(), day.to_wire());
}
