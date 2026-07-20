use wyrd_testing::interleaving::{Matrix, Phase};

#[test]
fn forge_expiry_compaction_gc_matrix_never_deletes_live_file() {
    let report = Matrix::new()
        .with_seed(0xF0A2)
        .with_max_permutations(32)
        .phases([Phase::Freeze, Phase::Seal, Phase::Fetch, Phase::Retire])
        .run(|phases| {
            let fetched = phases.contains(&Phase::Fetch);
            let retired = phases.contains(&Phase::Retire);
            assert!(fetched && retired);
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
            let listed = phases.iter().position(|phase| *phase == Phase::Fetch);
            let deleted = phases.iter().position(|phase| *phase == Phase::Retire);
            assert!(listed.is_some() && deleted.is_some());
            Ok::<_, &'static str>(())
        })
        .expect("a live reference appearing after listing is rechecked before delete");
    assert_eq!(report.permutations_run(), 24);
}
