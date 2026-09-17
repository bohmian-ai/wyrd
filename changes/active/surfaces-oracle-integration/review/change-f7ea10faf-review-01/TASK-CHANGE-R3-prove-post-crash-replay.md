---
id: TASK-CHANGE-R3
kind: remediation
status: ready
spec: SPEC-surfaces-oracle-integration
spec_revision: 9
requirements: [REQ-027C, REQ-029, AC-005]
depends_on: [TASK-CHANGE-R2]
parent_task: integrated-change
remediates: [CHANGE-002]
---

# Prove the restarted audit cycle actually replays the frozen batch

Implementation route: `$wyrd-implement`. The immutable reviewed target is
`f7ea10fafe772498ca0f5a0c63b783d578521d61` (tree
`e7cbd93b25e967ca4faa1966db6450a3a55ce969`), against base
`861f8d86cc3f9d7e70fb59489e80f8be62afddbf`. Read this directory's
`change-review.md`, approved spec revision 9, and the existing R2 task/evidence.
The owner waived literal Surfaces-parent ancestry and accepts previous broad
verification; the exact focused test below is the only required command for
this test-only follow-up.

## Required outcome

Keep the existing real-server journey
`crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs::frozen_audit_range_replays_once_while_its_tail_waits`.
It already proves tail-above-bound, two competing full publisher cycles,
`crashing`'s Scribe append followed by cancellation before settlement, and
the eventual 3+1 retained counts with empty staging. Close only the remaining
attribution gap: after cancelling `crashing`, make a surviving/restarted
`publish_tenant` cycle report its *own* Scribe append of the same frozen range
while the row fence still prevents settlement. Then release the fence and keep
the exact counts and drain assertions.

Reuse the existing `AuditPublisher::observe_appends` test-only signal for the
replay actor and order it so its append cannot happen before `crashing` is
confirmed cancelled. Keep its range read ahead of settlement: a replacement
cycle started only after the abort can be blocked at the chain head by another
settler holding that lock while waiting on the same row fence. The smallest
reliable approach is a test-support-only pause on the replay actor after it
reads the frozen range and before its Scribe ingest. Wait until it reaches that
pause, cancel `crashing`, release the pause, and require its own `Some(range)`
append signal before releasing the row fence. The two initially queued full
cycles remain the competing-publisher proof. Assert the bound remains live and
the replay excludes the tail. A mere `PublishOutcome::Published` return is not
enough: its empty-range path can return that value without a Scribe append if
another actor settles first. No ordinary production behavior, public API,
migration, dependency, or new test file is needed.

## Verification and handoff

Run exactly the focused nonzero selector under repository-managed Postgres:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=audit_publication::frozen_audit_range_replays_once_while_its_tail_waits)"'
```

Record the selected-test count, the crashing actor's append and abort, the
post-abort replay actor's append of the same range while fenced, final 3+1
counts, and empty staging. Do not rerun gate, its children, the full journey,
format, lints, or cloud tests solely for this remediation. Preserve unrelated
dirty files and historical reviews. Request a fresh cumulative
`$wyrd-change-review` after implementation and evidence are committed.

## Implementation evidence

Status: `IMPLEMENTED`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Two competing full cycles remain; `crashing` appends the frozen range and is cancelled before settlement | `audit_publication.rs::frozen_audit_range_replays_once_while_its_tail_waits`: `crashing` reports `Some(range)` via `observe_appends`; retained 3 frozen / 0 tail; aborted join `is_cancelled()`; bound still `range.seq_hi` | focused test | PASS |
| Replay actor's range read precedes settlement; its append cannot precede the crash | `survivor` uses test-support `AuditPublisher::pause_before_append` (after `read_range`, before Scribe ingest); test waits for the pause, asserts `survivor` has appended `None`, then cancels `crashing`, then adds the release permit | focused test | PASS |
| Post-abort replay reports its own append of the same range while fenced; bound live; tail excluded | `survivor_appended` observed `Some(range)`; `survivor` not finished; `frozen_bound == Some(range.seq_hi)`; retained 3 frozen / 0 tail — all before `release_fence` | focused test | PASS |
| Settlement, final counts, empty staging | after fence release `survivor` returns `Published` of exactly the frozen range; retained 3 frozen then tail 1, frozen still 3; `await_drained` empty | focused test | PASS |

Test-only seam: `crates/wyrd/wyrd-server/src/audit/publication.rs` adds a
`#[cfg(feature = "test-support")]` `append_pause` field (arrival watch plus a
zero-permit semaphore), `pause_before_append`, and the wait after
`read_range`. Nothing compiles in without that feature; the server's own sweep
instance never sets it. No ordinary production behavior, public API,
migration, dependency, or new test file was added.

Verification (repository-managed Postgres), final tree:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=audit_publication::frozen_audit_range_replays_once_while_its_tail_waits)"'
```

Six consecutive runs, each `1 test run: 1 passed, 11 skipped` (12.0–13.0 s).
`git diff --check` clean. Gate, full journey, format lane, lints, and cloud
tests were not rerun, per the task. Unrelated dirty files and other review
directories untouched. Next: fresh cumulative `$wyrd-change-review`.
