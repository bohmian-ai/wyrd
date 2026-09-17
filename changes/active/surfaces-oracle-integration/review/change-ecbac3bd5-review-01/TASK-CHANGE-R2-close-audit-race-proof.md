---
id: TASK-CHANGE-R2
kind: remediation
status: ready
spec: SPEC-surfaces-oracle-integration
spec_revision: 9
requirements: [AC-005]
depends_on: [TASK-CHANGE-R1]
parent_task: integrated-change
remediates: [CHANGE-002]
---

# Prove the combined real-server audit race

Implementation route: `$wyrd-implement`.

## Authority and subject

- Approved `changes/active/surfaces-oracle-integration/spec.md` revision 9 and the explicit owner waiver of literal Surfaces-parent ancestry.
- Cumulative base `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`; reviewed target `ecbac3bd54be7504d2c6eacf71ac5c3c5fe2cd99`, tree `a08375474672fa330748bb2918b4a89e169c1008`.
- Finding `CHANGE-002` in this directory's `change-review.md`; prior `TASK-CHANGE-R1` in `review/change-ee63cba17-review-01/`.
- TASK-001/005/006 have committed human approval. Preserve historical verdicts; do not rerun those task-review cycles.

## Outcome

Close the real-server proof gap in the existing `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs::frozen_audit_range_replays_once_while_its_tail_waits` journey. This is a test-only remediation; the finding establishes no production publisher, SQL, query, SDK, or storage defect.

The current test commits a tail above a frozen range and verifies a direct SQL competitor reuses the upper bound. That competitor never runs `AuditPublisher::publish_tenant`. The test's retained-row poll can also be satisfied by the server's background publisher, so aborting its explicit spawned cycle does not prove a crash after that cycle's durable append.

Retain the current fence, tail-before-settlement ordering, and eventual 3+1 retained-row/idle-drain checks. Drive two actual `AuditPublisher::publish_tenant` cycles against the same live bound while the tail exists. Make the ordering observable: identify a specific cycle that completed Scribe's durable append yet remains blocked before settlement, abort that cycle, and show the surviving/restarted cycle reuses the frozen range rather than widening it to include the tail. Require the original three decisions exactly once, the tail once in its later range, and staging drained to zero. Keep any direct SQL freeze as supporting seam evidence, not as the claimed second publisher.

Use existing test-support observation and the existing Postgres row fence where they suffice. If actor identity cannot be established from them, add only a narrow test-support signal at the existing publisher seam after successful Scribe append and before settlement; never expose production partial-stage methods or add a second harness/test file. Preserve the server's background sweep semantics rather than disabling production behavior to make the test pass. No public contract, migration, source-branch, or production-code change is authorized.

## Verification

Run the exact existing test with nonzero selection under repository-managed Postgres:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=audit_publication::frozen_audit_range_replays_once_while_its_tail_waits)"'
```

Then run `mise run test:bifrost:journey:server`, `mise run fmt`, `mise run lints`, and `git diff --check 861f8d86cc3f9d7e70fb59489e80f8be62afddbf..HEAD`. Record which actor appended, which was aborted, the competing bound, replay/retained counts, and zero staging rows. The `mise run gate` launched on `ecbac3bd5` had not finished when this task was written; do not count a killed or incomplete run as passing. The owner accepts same-tree gate-child equivalence, though no lane may be skipped or zero-selected. Earlier local `mise.local.toml` S3/GCS/Azure proof is accepted for unchanged production source; recheck final-tree verification scope after the test edit.

After implementation and evidence are committed, request a fresh `$wyrd-change-review` of the cumulative base-to-target range. Do not complete, merge to main, push, release, or deploy from this task.

| Acceptance criterion | Finding |
|---|---|
| The existing real-server journey deterministically proves two full publisher cycles contend under a frozen bound with a live tail, abort-after-append replay, exact retained counts, and idle drain. | `CHANGE-002` |
