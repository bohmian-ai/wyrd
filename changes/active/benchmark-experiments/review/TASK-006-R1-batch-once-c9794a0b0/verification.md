# Verification rerun by the orchestrator (candidate `c9794a0b0`, sequential, no other cargo)

| Proof | Command (selector) | Result |
|---|---|---|
| P1 | `cargo nextest run --locked -p wyrd-queue --lib -E 'test(=variant::tests::validation_only_matches_owned_validation)'` | 1/1 pass |
| P1/P2/P4/attribution | `-p vala-bifrost-redux --lib`: `tables::tests::builtin_registry_dispatches_canonical_value_validation`, `tables::staged::tests::staged_rows_project_into_one_dense_batch`, `scribe::parquet_writer::tests::{gathered_sort_matches_concatenated_reference, gathered_sort_respects_persistence_admission, generation_encoding_preserves_sort_tenant_and_artifact_identity, parquet_writer_honors_sort_order, tenant_binding_mismatch_fails_before_encoding}`, `gate::attribution::tests::batch_card_uids_match_the_encoded_stream` | 8/8 pass |
| P2/P3 | `-p wyrd-server --features test-support --lib`: `scribe_outbox::tests::{dense_projection_preserves_records_and_attribution, local_canonical_and_peer_ipc_preserve_frames_and_replay, staged_writes_charge_their_owned_memory}` | 3/3 pass |
| P3 real WAL | `scripts/postgres/with-test-postgres.sh` + `-p vala-bifrost-redux --lib -E 'test(=scribe::scribe_persistence_path::canonical_nested_batches_share_one_managed_wal_path)'` | 1/1 pass |
| P4 journeys | `with-test-postgres.sh` + `-p wyrd-testing --test scribe -P journey --run-ignored=all` `cross_shard::scribe_cross_shard_generations_publish_packed_objects_once`, `source_boundary::scribe_local_stage_to_hot_source_transition_is_exact` | 2/2 pass |
| Boundaries | `cargo fmt --all --check`; `git diff --check 55f87cef1...c9794a0b0` | exit 0 |

Note: the evidence section's selector `scribe::tests::scribe_persistence_path::canonical_nested_batches_share_one_managed_wal_path`
matches 0 tests (nextest exit 4); the real name is `scribe::scribe_persistence_path::...` as written in the task's P3 block.

Not rerun: L1 benchmarks (P5) per caller constraint; records under `benchmarks/` and in the task evidence are taken as recorded.
Lints/docs:check/check:deps/check:tenant-isolation not rerun; carried from the task evidence.
