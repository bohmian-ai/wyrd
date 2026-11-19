# Verification evidence and limits

Review subject and exclusions are in [subject.md](subject.md). Source/authority
and packet hashes were checked before discovery and during review and matched.
`git diff --check` passes on the submitted working tree. No production, test,
task or architecture file is edited during review; report files are the only
new review output. No staging, commits or pushes occur.

## Available execution evidence

The submitted R5 task's Implementation evidence records:

- P1/P2/P4/P5 failures on the older R4 source with stated compile-only test
  adaptations; P3/P6 regression passes on that source;
- final R5 runtime/durable-work/groups integration modules: 83/83;
- final SQL executions integration module: 34/34, including claim fencing;
- T5 PASS, and P1–P6/T3/T6 included in recorded server runs;
- fmt/lints/tenant-isolation/codegen/diff checks PASS after redundant token
  argument removal, plus wyrd-testing compilation;
- explicit omission of the P3 reserved-capacity interruption case;
- repeated selected-result delivery retains multiple physical rows with the
  same selected identity; identical sealed-batch replays remain deduplicated.

These are implementation-recorded executions, not independently rerun results
on this pinned candidate. Reviewers independently inspect their assertions,
owners and applicability. In particular, a green test name does not establish
an unexercised scenario. The older R4 review verification and root's prior
focused testing are historical and do not substitute for this candidate's
closure evidence. No broad gate, full language journeys, new benchmark or
synthetic host load is run for this focused review.

All agents are read-only with respect to the subject and run no Cargo commands.
Any further execution must be serialized by the orchestrator and recorded here.

## Independent focused execution

After the data specialist identified a specific library-plan assertion absent
from the reported SQL integration module, the orchestrator confirmed no active
Cargo process and ran this one existing test with tracing, serially:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_sql=debug cargo nextest run --locked -p wyrd-sql --lib -E "test(=queries::verifier_executions::pg_tests::runnable_scan_walks_the_claim_indexes_however_large_the_backlog)"'
```

Wyrd and Vala migration apply/idempotence each pass 1/1. Nextest run
`66a739c4-bda9-4bdd-976c-bc720dc77d71`: one test selected, one FAIL. The trace's
assertion at `verifier_executions.rs:4398:13` rejects
`"Index Only Scan" via "verifier_executions_delivery_due"`.
The query retains the owed-delivery index walk, while the changed assertion
allows only the two computation-claim indexes. The data specialist independently
traces that producer/assertion mismatch and supplies its correction; the
orchestrator changes no code to satisfy the test.
