# R6 verification

Subject/authority and source/packet pins are in [subject.md](subject.md).
Only review reports are written. No source/test/task/authority changes, staging,
commits or pushes occur. `git diff --check` passes before focused execution.

## Recorded implementation evidence

The submitted R6 task records RED/GREEN for keyed Result framing and independent
result redelivery across restart, a mutation check for the capacity refusal
observation, actual hosted A/B→C execution/reclaim, restored SQL plan acceptance
and local rustdoc closure. Recorded owner runs are 28/28 state/outbox units and
84/84 related server integration tests, with fmt/lints/diff checks clean.
These are implementation-recorded results; discovery reviewers independently
read the assertions and production paths rather than infer acceptance from counts.
Earlier/background session logs are not attributed to this candidate without
matching source and command evidence.

## Independent focused execution

The orchestrator confirmed no active Cargo builds before running these serially.
No reviewer runs Cargo. Integration uses a fresh isolated repository PostgreSQL
lifecycle, avoiding unrelated existing runtime/session activity.

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::result_redelivery_keeps_batch_identity)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_server=debug cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::result_redelivery_is_idempotent_after_restart) | test(=pg_verification_runtime::cancelled_selected_delivery_waits_for_all_required_acks) | test(=pg_verification_runtime::lost_result_ack_replays_the_identical_sealed_batch_and_scribe_deduplicates) | test(=pg_verification_runtime::exhausted_reconciliation_survives_source_and_capacity_outage) | test(=pg_durable_verifier_work::concurrent_settlements_release_their_hosted_dependent)" && WYRD_LOG=info,wyrd_sql=debug cargo nextest run --locked -p wyrd-sql --lib --test-threads=1 -E "test(=queries::verifier_executions::pg_tests::runnable_scan_walks_the_claim_indexes_however_large_the_backlog)"'
```

Unit nextest `d52b5c3c-ef87-4c49-aae2-51f951f6f761`: one registered test, PASS.
Wyrd migration apply/idempotence `dbb41592-9e43-4a93-b10b-848048915db7`:
one test, PASS. Vala migration apply/idempotence
`7890c18d-2f89-44f5-9bcb-c839ff45a984`: one test, PASS.

Server integration nextest `78c35472-5941-4261-8e92-7123fd774ade`: five
registered tests, all PASS:

- real hosted concurrent A/B→C and recovery;
- cancelled selected delivery waiting for all result ACKs;
- exhausted reconciliation with separate source and capacity outage;
- identical sealed-batch lost-ACK retry deduplication;
- independently restaged result idempotency after restart, partial ACK and
  conflicting rewrite refusal.

SQL library nextest `fe1c6de4-5bad-4adc-b223-1397c4c2e15e`: one registered
tenant-scan plan test, PASS. All focused commands completed with exit code zero.
No source was changed to obtain these results.

No broad aggregate gate, language journeys, new benchmark or synthetic host load
is requested for this bounded remediation review. Prior source-backed evidence
retains its stated limits; full integrated change verification remains separate.
