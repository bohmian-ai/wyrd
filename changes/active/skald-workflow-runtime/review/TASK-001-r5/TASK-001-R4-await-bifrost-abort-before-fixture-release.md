# TASK-001-R4 — Await Bifrost abort before fixture release

## Authority and immutable subject

- Approved specification:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 11
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Parent remediation:
  `changes/active/skald-workflow-runtime/review/TASK-001-r4/TASK-001-R3-close-round-four-review-gaps.md`
- Review verdict:
  `changes/active/skald-workflow-runtime/review/TASK-001-r5/verdict.md`
- Validated ledger:
  `changes/active/skald-workflow-runtime/review/TASK-001-r5/findings-validation.md`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Reviewed candidate: `09e4b82c2a4cab3ea27e0889d3acf3ea22c2b596`
- Finding: `FIND-TASK-001-28`

Implementation skill: `$wyrd-implement`.

## Outcome

Finish the existing test-harness teardown correction by making completed abort,
not abort signalling, the fence before runtime owners and `PgFixture` are
released. Preserve the closed documentation, telemetry, Workflow, security,
and bound serve-task behavior.

## Diagnosis

### FIND-TASK-001-28 — Teardown records settlement before abort completes

- Violated obligation: every database-using role or serve task owned by
  `WyrdTestServer` must complete or be explicitly aborted before runtime owners
  and the fixture database are released, including implicit drop.
- Current behavior: `WyrdTestServer::settle_lifecycle` sets
  `bifrost_settled = true` before the Bifrost owner settles. It gives graceful
  `Bifrost::shutdown` one deadline, then wraps fallback `Bifrost::abort` in the
  same deadline. Active-runtime `Drop` supplies zero, and an ordinary graceful
  attempt may consume the deadline before fallback begins.
- Exact evidence:
  `crates/wyrd/wyrd-testing/src/server.rs:831-846,937-948,986-992,3675-3708,5479-5505`;
  completion contracts at
  `crates/wyrd/wyrd-server/src/state.rs:1990-2001,2062-2089` and
  `crates/vala/vala-bifrost-redux/src/storage/mod.rs:1041-1058`;
  forced database removal at
  `crates/shared/wyrd-dev-fixtures/src/pg.rs:426-475`.
- Root cause: `BifrostStorage::abort` deliberately has no graceful deadline.
  It cancels owners and waits for retained loaders and governed requests to
  reach zero. Reapplying the expired graceful deadline can poll that future
  once, cancel it while pending, log a warning, and return. The already-written
  settled flag suppresses another attempt.
- Observable consequence: live governed storage or role work can overlap
  dedicated runtime release and `DROP DATABASE ... WITH (FORCE)`, retaining the
  database-after-drop or Oracle self-fence/process-abort race this remediation
  was created to remove.
- Why current proof falls short: the in-process teardown test drops the server
  synchronously off-runtime with idle storage. Abort finishes on its first poll,
  so the test does not reach the active-runtime zero-budget or delayed-storage
  path. The bound stalled-serve test does close the detached-handle portion and
  must remain.

The reported fact that in-process teardown normally selects abort is expected.
Only `BoundServer::run` owns and joins Forge supervision and can certify it as
drained; an in-process harness must not fabricate that certification.

## Decision-complete correction

Keep the correction in the existing `WyrdTestServer::settle_lifecycle` owner.
Retain the current budget for graceful serve-task and Bifrost drain attempts.
When Bifrost has not produced a clean drain within that budget, await the
existing `Bifrost::abort` completion fence without applying the expired grace
deadline. A zero budget skips graceful Bifrost drain and awaits abort directly.
Record `bifrost_settled` only after a clean bound/drain report or after abort
has completed.

This is the smallest safe source correction because every explicit shutdown,
startup rollback, abrupt/terminal seam, and implicit drop routes through the
same producer. Consumer-specific guards would duplicate the invariant and
still leave sibling callers exposed. Reuse the existing
`StorageOperationBarrier` and governed-request settlement path for proof; no
new lifecycle abstraction or test harness is needed.

## Constraints and preserved behavior

- Preserve bound serve-task timeout abort-and-join and panic propagation.
- Preserve production `BoundServer::run` as the sole Forge supervision-drain
  authority; do not mark in-process Forge supervision drained.
- Preserve production Bifrost shutdown, abort, storage, self-fencing, and
  recovery semantics and public APIs.
- Preserve `WyrdTestServerInner` field order and fixture-last destruction.
- Preserve closed `FIND-TASK-001-27` and `FIND-TASK-001-29`, Revision 11
  Observer deletion, Workflow behavior, ExtGateway security, and SDK surfaces.
- Do not add a sleep, retry, larger timeout, configuration surface, crate,
  dependency, feature, production lifecycle API, or second shutdown owner.
- Do not weaken, ignore, or replace the existing tests or gates.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-001-28` | Graceful serve/Bifrost work remains bounded; every fallback waits for the existing Bifrost abort fence to complete; settlement is recorded only afterward; active-runtime implicit drop cannot release the fixture while a real governed storage operation remains live. |

The existing bound stalled-serve case must continue to prove that an overdue
serve task is aborted and joined rather than detached. The in-process proof,
`dropping_an_in_process_server_with_live_storage_work_awaits_abort_before_fixture_release`,
must deterministically admit a real governed storage operation at the existing
`StorageOperationBarrier`, observe that barrier before teardown, and exercise
implicit drop on an active Tokio runtime. It must prove teardown cannot return
or release the fixture while storage remains unsettled, then assert operation
termination and storage settlement precede fixture-release observation. Use
synchronization and existing inspection/owner state, not sleeps.

## Focused proof and broader verification

Run both exact selectors through the repository-managed Postgres wrapper.
Confirm each selector names and runs one test; do not rely on a positional
filter that can select none.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && \
  mise exec -- cargo nextest run --locked -p wyrd-testing --lib \
  -E 'test(=server::teardown_tests::shutdown_aborts_and_joins_a_serve_task_that_outlives_its_drain) | test(=server::teardown_tests::dropping_an_in_process_server_with_live_storage_work_awaits_abort_before_fixture_release)'"
```

Then run:

```bash
mise run fmt
mise run lints
mise run test:wyrd
git diff --check
```

A red required lane blocks completion. Route this task directly to
`$wyrd-implement`; the next immutable review reassesses the complete original
base-to-candidate range.
