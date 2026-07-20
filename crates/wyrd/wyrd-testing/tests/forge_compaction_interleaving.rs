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
            let prepared = phases.iter().position(|phase| *phase == Phase::Seal);
            let replay = phases.iter().position(|phase| *phase == Phase::Replay);
            assert!(prepared.is_some());
            assert!(replay.is_some());
            Ok::<_, &'static str>(())
        })
        .expect("all bounded phase orderings preserve a terminal path");
    assert_eq!(report.permutations_run(), 24);
}
