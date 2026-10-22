# Review-time focused verification

Candidate: `339c9b8d0484421b5fd67b71194eed8fcc60cadb`; tracked source unchanged. The orchestrator ran these commands sequentially. Discovery agents ran no Cargo processes. Passing checks support only their exercised paths and are not a finding-closure verdict.

```bash
WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::a_retired_generation_stays_charged_until_its_last_holder_drops) | test(=scribe::tail_rpc::tests::a_capture_elapsed_before_installation_is_refused_and_retains_nothing) | test(=scribe::tail_rpc::tests::an_elapsed_capture_refuses_a_new_open_before_its_expiry_runs)'
```

Exit 0; 3 selected, 3 passed, 838 skipped. Nextest run `da3af7d1-a49f-4826-81a0-e099b184c995`.

```bash
WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=oracle::tail_discovery::tests::a_stalled_release_settles_by_the_query_deadline)'
```

Exit 0; 1 selected, 1 passed in 1.031 seconds, 562 skipped. Nextest run `ae23b747-f9b6-47ff-a0e4-bd6f4dad2ad3`.

```bash
git diff --check 1c5edf72958af8f47c63eb8ce39eb316672e170a...339c9b8d0484421b5fd67b71194eed8fcc60cadb
```

Exit 0. No broad/static lane, benchmark or integrated client/agent suite was rerun in this review. R1's own recorded commands/evidence remain inputs for independent coverage assessment; original failed capacity is not qualified by these focused checks.
