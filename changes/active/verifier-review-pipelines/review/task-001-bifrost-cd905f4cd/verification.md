# Independent verification

Candidate: `cd905f4cd929785bda25696326d58b4a4ae88dee`.
Commands run sequentially by the orchestrator. No implementation edits.

| Command | Observed result |
| --- | --- |
| `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^verification::state::tests::/) \| test(/^scribe_outbox::tests::/)'` | PASS: 25/25, 558 skipped. Includes sibling progress, coalescing/cancelled waiters, duplicate terminal selection, refused revision, own-frame acknowledgement and execution lane coverage. |
| `mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::checkpoints_restore_settled_blocks_and_count_interrupted_attempts)'` | PASS: 1/1, 59 skipped. This closes the reported post-Revision-1 rerun gap for the unchanged pure engine checkpoint regression. |

Server integration verification below uses repository-managed isolated
Postgres and the exact server integration target. Existing implementation evidence remains
separate from independently reproduced evidence. Performance measurements are
not inferred from green tests.

## Server integration result

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(/^pg_durable_verifier_work::/) | test(/^pg_verification_runtime::/) | test(/^pg_verifier/)"'
```

PASS: 68/68, 270 skipped, 416.880 seconds. Includes all 12 durable verifier
work integration tests and the verification runtime/verifier slices that had
not been rerun cleanly after the final change. Setup also passed both the Wyrd
and Vala migration apply/idempotence tests in the isolated repository-managed
Postgres lifecycle. These setup tests do not exercise legacy progressed-row
migration refusal. The isolated lifecycle completed successfully.

No discovered failure scenario was implemented as a new test during review;
source-backed findings remain independently validated separately. No throughput
or encoded-byte benchmark was run. This task review does not repeat integrated
language journeys or broad aggregate gates.

## Static checks and immutability

- `mise run fmt:check`: PASS.
- `git diff 0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb...cd905f4cd929785bda25696326d58b4a4ae88dee --check`: PASS.
- Candidate HEAD and the recorded authority SHA-256 hashes remained unchanged
  after integration verification. No production/test/generated source edits.
- Clippy/codegen/docs/boundary checks were not rerun in this review; their
  historical evidence and static generator/standards checks are recorded by
  the task and selected repository reviewer. No broader gate is claimed.
