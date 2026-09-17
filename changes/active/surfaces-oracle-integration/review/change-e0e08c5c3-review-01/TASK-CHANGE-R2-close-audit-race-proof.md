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

# Close real-server publication race proof

Implementation route: `$wyrd-implement`.

## Authority and immutable subject

- Approved `changes/active/surfaces-oracle-integration/spec.md` revision 9; exact AC-005 real-server obligation.
- Original TASK-001 through TASK-006, committed human approval of TASK-001/005/006, committed TASK-003-R4 PASS for TASK-002/003/004, and committed TASK-CHANGE-R1 evidence.
- Integrated review: this directory's `change-review.md`.
- Cumulative base `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`; reviewed target `e0e08c5c3b53f4714b880e5242dba437a9da3473`, tree `6ce48feabb6755dea59b8d8b00b1eb1fad7eec23`.
- User waived literal Surfaces-parent ancestry and accepts the previously recorded verification. For this test-only follow-up, the user requires only the focused test below; no repeat gate or broader lane is required.

## Required outcome

In the existing `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs::frozen_audit_range_replays_once_while_its_tail_waits` journey, prove the AC-005 interleaving with two **full** `AuditPublisher::publish_tenant` cycles, not merely one cycle plus a direct SQL freeze. The tail must commit above a still-live frozen upper bound. Identify one cycle that durably appended its frozen range to Scribe, abort that same cycle while settlement remains blocked by the existing row fence, and show a surviving/restarted cycle reuses the bound with the tail excluded. Assert the original three decisions exactly once, the tail once in its later range, and idle staging empty.

Retain the current SQL-freeze test seam as supporting evidence if useful. Reuse the current Postgres harness and row fence. If actor identity is otherwise unobservable, add only a narrow test-only signal after Scribe append and before settlement at the existing publisher seam; no production partial-stage API, new harness/test file, or production behavior change. Preserve background sweep semantics and tenant isolation.

## Verification and handoff

Run the exact focused nonzero selector under repository-managed Postgres:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=audit_publication::frozen_audit_range_replays_once_while_its_tail_waits)"'
```

This focused test is the only required command for this test-only remediation. Do not rerun `gate`, its child tasks, the full server journey, format, lints, or cloud tests solely for this change. Record the actor that appended, the actor aborted, bound reuse, retained counts, zero staging, and the focused test result. An incomplete run or empty selection is not a pass. Request a fresh cumulative `$wyrd-change-review` after implementation and evidence are committed.

No production code, public contract, migration, dependency, source-branch rewrite, main merge, push, release, or deployment is requested. Preserve unrelated dirty files and historical review records.

## Implementation evidence

Status: `IMPLEMENTED`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Two full `publish_tenant` cycles against a live bound with the tail staged above it | `audit_publication.rs::frozen_audit_range_replays_once_while_its_tail_waits`: `survivor` and `crashing` production cycles are spawned and counted as lock-blocked (`await_blocked_backends(&mut freezer, 3)`, with the competitor SQL freeze) before the tail-bearing freeze commits | focused test | PASS |
| Actor that appended is identified | `crashing` alone carries the test-support `AuditPublisher::observe_appends` signal, fired in `publish_range` only after Scribe `ingest_frame` returns; asserted to report exactly `Some(range)`; retained history shows 3 frozen / 0 tail | focused test | PASS |
| Same actor aborted while settlement is fenced | with the row fence held, at least one backend is lock-blocked, neither cycle is finished, `crashing.abort()` and its join reports `is_cancelled()` | focused test | PASS |
| Bound reuse, tail excluded | `frozen_bound` reads `publishing_seq_hi == range.seq_hi` after the abort; after the fence release `survivor` returns `PublishOutcome::Published { seq_lo, seq_hi }` equal to the frozen range; competitor SQL freeze still returns `Some(range)` | focused test | PASS |
| Retained counts and zero staging | frozen decisions retained exactly 3 before and after replay, tail retained 0 then 1, `await_drained` empty | focused test | PASS |

Test-only seam: `crates/wyrd/wyrd-server/src/audit/publication.rs` gains a
`#[cfg(feature = "test-support")]` watch sender field, `observe_appends`, and
its post-append send. Without that feature nothing compiles in; the server's
own sweep instance never sets it, so sweep semantics and tenant isolation are
unchanged. No production partial-stage API, harness, new test file, migration,
dependency, or public contract was added.

Supporting fix found during verification: `pg_stat_activity` is snapshotted
per transaction, so the blocked-backend poll calls `pg_stat_clear_snapshot()`
each iteration; before that, one of six runs timed out (`only 2 of 3 backends
blocked`) because a later-opened pooled connection was invisible.

Verification (repository-managed Postgres):

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=audit_publication::frozen_audit_range_replays_once_while_its_tail_waits)"'
```

Result on the final tree: six consecutive runs, each `1 test run: 1 passed,
11 skipped` (12.2–16.6 s). `git diff --check` clean. Per the task, no gate,
full journey, format lane, lints, or cloud tests were rerun. Unrelated dirty
files and other review directories were left untouched. Next: fresh cumulative
`$wyrd-change-review`.
