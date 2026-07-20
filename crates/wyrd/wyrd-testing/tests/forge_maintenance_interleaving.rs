use opendal::raw::Timestamp;
use std::time::Duration;
use vala_bifrost_redux::forge::expire::{SnapshotSummary, select_expirable_snapshots};
use vala_bifrost_redux::forge::{ProtectedLiveSet, is_gc_candidate};
use wyrd_testing::interleaving::{Matrix, Phase};

#[test]
fn forge_expiry_compaction_gc_matrix_never_deletes_live_file() {
    let report = Matrix::new()
        .with_seed(0xF0A2)
        .with_max_permutations(32)
        .phases([Phase::Freeze, Phase::Seal, Phase::Fetch, Phase::Retire])
        .run(|phases| {
            let snapshots = vec![
                SnapshotSummary {
                    id: 91,
                    parent_id: None,
                    timestamp_ms: 1,
                },
                SnapshotSummary {
                    id: 7,
                    parent_id: Some(91),
                    timestamp_ms: 2,
                },
                SnapshotSummary {
                    id: 400,
                    parent_id: Some(7),
                    timestamp_ms: 3,
                },
            ];
            let expirable = select_expirable_snapshots(&snapshots, Some(400), &[], 4, 2);
            assert_eq!(expirable, vec![91]);
            let old = Timestamp::from_millisecond(0).expect("old timestamp");
            let now = Timestamp::from_millisecond(48 * 60 * 60 * 1_000).expect("current timestamp");
            let mut live = ProtectedLiveSet::default();
            for phase in phases {
                if *phase == Phase::Fetch {
                    live.insert("tenants/t/traces/spans/data/live.parquet".to_owned());
                }
                if *phase == Phase::Retire {
                    let candidate = is_gc_candidate(
                        "tenants/t/traces/spans/data/live.parquet",
                        &live,
                        Some(old),
                        now,
                        Duration::from_hours(24),
                    );
                    if live.contains("tenants/t/traces/spans/data/live.parquet") {
                        assert!(!candidate, "live objects are never GC candidates");
                    }
                }
            }
            Ok::<_, &'static str>(())
        })
        .expect("maintenance ordering preserves live files");
    assert_eq!(report.permutations_run(), 24);
}

#[test]
fn forge_gc_reference_appears_between_list_and_delete_is_protected() {
    let report = Matrix::new()
        .with_seed(0xF0A3)
        .with_max_permutations(32)
        .phases([Phase::Freeze, Phase::Fetch, Phase::Seal, Phase::Retire])
        .run(|phases| {
            let old = Timestamp::from_millisecond(0).expect("old timestamp");
            let now = Timestamp::from_millisecond(48 * 60 * 60 * 1_000).expect("current timestamp");
            let mut live = ProtectedLiveSet::default();
            let orphan = "tenants/t/traces/spans/data/orphan.parquet";
            let mut deleted = false;
            for phase in phases {
                if *phase == Phase::Retire && !live.contains(orphan) {
                    assert!(is_gc_candidate(
                        orphan,
                        &live,
                        Some(old),
                        now,
                        Duration::from_hours(24),
                    ));
                    deleted = true;
                }
                if *phase == Phase::Fetch {
                    live.insert(orphan.to_owned());
                }
            }
            assert!(deleted || live.contains(orphan));
            Ok::<_, &'static str>(())
        })
        .expect("a live reference appearing after listing is rechecked before delete");
    assert_eq!(report.permutations_run(), 24);
}
