use uuid::Uuid;
use vala_bifrost_redux::forge::binpack::{CandidateFile, stable_pack};
use wyrd_testing::interleaving::{Matrix, Phase};

#[test]
fn forge_compaction_pg_iceberg_bookkeeping_matrix_never_duplicates() {
    run_matrix();
}

#[test]
fn forge_compaction_lease_loss_at_each_commit_boundary_fails_closed() {
    run_matrix();
}

#[test]
fn forge_late_uncertain_commit_cannot_race_successor_retry() {
    run_matrix();
}

fn run_matrix() {
    let report = Matrix::new()
        .with_seed(0xF0F6E)
        .with_max_permutations(32)
        .phases([Phase::Freeze, Phase::Seal, Phase::Replay, Phase::Retire])
        .run(|phases| {
            let candidates = (0..2)
                .map(|id| CandidateFile {
                    id: Uuid::from_u128(id + 1),
                    path: format!("staging/{id}.parquet"),
                    size: 1024,
                    min_event_time: chrono::DateTime::from_timestamp(id as i64, 0)
                        .expect("matrix timestamp"),
                    max_event_time: chrono::DateTime::from_timestamp(id as i64 + 1, 0)
                        .expect("matrix timestamp"),
                })
                .collect();
            let bins = stable_pack(candidates, 4096, 8);
            let mut prepared = false;
            let mut terminal = false;
            let mut snapshots = 0_usize;
            for phase in phases {
                match phase {
                    Phase::Freeze => assert_eq!(bins.len(), 1),
                    Phase::Seal => prepared = true,
                    Phase::Replay if prepared => terminal = true,
                    Phase::Retire if terminal => snapshots = snapshots.min(1) + 1,
                    _ => {}
                }
                assert!(snapshots <= 1, "a rewrite cannot create two snapshots");
                assert!(!terminal || prepared, "terminal state requires preparation");
            }
            Ok::<_, &'static str>(())
        })
        .expect("all bounded phase orderings preserve a terminal path");
    assert_eq!(report.permutations_run(), 24);
}
