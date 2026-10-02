# TASK-001 round-five system-resilience review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `09e4b82c2a4cab3ea27e0889d3acf3ea22c2b596`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 11,
  including `REQ-053`
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Reviewed remediation:
  `changes/active/skald-workflow-runtime/review/TASK-001-r4/TASK-001-R3-close-round-four-review-gaps.md`
- Prior closure hypotheses: `FIND-TASK-001-27`, `FIND-TASK-001-28`, and
  `FIND-TASK-001-29`

The candidate remained at the stated commit during this review. CodeGraph is
not present. I reviewed the cumulative base-to-candidate change, the applicable
Bifrost lifecycle authority, the production and harness lifecycle owners, and
the recorded verification evidence. This report addresses system resilience;
the other two remediation findings have no material deployment or recovery
effect.

## Deployed-path evidence

### Bound server

`WyrdTestServerBuilder::bind` retains the production `BoundServer::run` task in
`serve_handle` (`crates/wyrd/wyrd-testing/src/server.rs:3607-3654`). Production
serving owns the Scribe lifecycle scanner and publisher, Forge scheduler and
worker, listeners, and other process tasks in one `JoinSet`
(`crates/wyrd/wyrd-server/src/app/server.rs:502-748`). On shutdown it closes
readiness, drains or aborts that set, records Forge supervision completion, and
then performs the ordered Bifrost drain or abort
(`crates/wyrd/wyrd-server/src/app/server.rs:749-887`).

The remediation correctly retains a timed-out serve handle, calls
`abort()`, and awaits its join before fixture release
(`crates/wyrd/wyrd-testing/src/server.rs:807-829`). This closes the prior
detached-task path. A clean bound return is also recorded so the harness does
not attempt a second Bifrost drain (`server.rs:809-812,920-924,1052-1055`).

### In-process server

`start_in_process` composes the production Scribe, Forge, Oracle, storage, and
dedicated runtime owners, but it creates no `BoundServer::run` task
(`crates/wyrd/wyrd-testing/src/server.rs:4237-4670`). In particular, the Forge
scheduler and worker loops are spawned only inside `BoundServer::run`
(`crates/wyrd/wyrd-server/src/app/server.rs:622-640`). Therefore the reported
residual fact is accurate: the Forge `supervision_drained` bit starts false and
is only written after production supervision joins (`state.rs:1502-1504,
1572-1579`; `app/server.rs:780-782`), so an in-process call to
`Bifrost::shutdown` refuses to claim a graceful Forge drain and enters its
abort-on-failure path (`state.rs:1990-2020`).

That refusal is the correct fallback, not itself a leaked Forge supervisor:
there is no in-process Forge supervisor to join. It also preserves the
production invariant that only the real supervisor may certify a clean Forge
drain. The remediation explicitly permits in-process work to complete **or be
aborted**, so always selecting abort in this harness mode is within scope.

## Failure and recovery paths

| Failure or transition | What stops | What remains/recovery | Assessment |
|---|---|---|---|
| Clean bound shutdown | Admission and readiness close; the production `JoinSet` joins; Bifrost drains in Oracle/Scribe/storage order. | Durable Scribe residue remains recoverable by WAL/staging rules. | PASS |
| Bound serve task exceeds the harness budget | The retained serve task is aborted and joined. The Bifrost fallback then runs because no clean report was returned. | No detached serve task can continue against the fixture. | PASS for the prior detached-task defect. |
| In-process teardown | No process supervisor exists. `Bifrost::shutdown` observes unmarked Forge supervision, refuses a clean report, and calls the role/storage abort path. | Scribe durable work is left for recovery; the fixture is disposable. | The abort choice is acceptable, but its completion is not guaranteed by the harness; see `SYS-R5-001`. |
| Graceful Bifrost shutdown or its internal abort reaches the two-second deadline | The outer `timeout_at` cancels the `Bifrost::shutdown` future. The fallback `Bifrost::abort` receives the same already-expired deadline. | A still-pending storage abort can be cancelled; teardown nevertheless records `bifrost_settled = true` and releases runtime owners and the fixture. | FAIL — false settlement remains reachable. |
| Process restart after an actual abort | Scribe WAL/staged evidence remains authoritative; no clean drain is claimed. | Normal boot replay/reconciliation restores durable ownership. | PASS when abort actually completes. |

The Bifrost authority requires abort to leave no retained loader or governed
storage operation running. Its implementation deliberately awaits abort
without another deadline because the graceful close already consumed the
deadline (`crates/vala/vala-bifrost-redux/src/storage/mod.rs:1041-1058`). The
harness currently cuts that owner off with another timeout instead.

## Affected capabilities

The remaining path is test-harness-only; production `BoundServer::run`
semantics are unchanged. It affects every journey using an in-process
`WyrdTestServer`, plus bound startup/teardown fallback paths that reach the
harness-owned Bifrost abort after the serve task fails or times out. The
observable risk is the same class as prior `FIND-TASK-001-28`: fixture or
dedicated-runtime destruction can proceed while a Bifrost storage operation is
still settling, making test outcomes scheduling-dependent and allowing work to
outlive resources the harness claims are quiescent.

## Material proposed finding

### SYS-R5-001 — The fallback can release the fixture after cancelling the required Bifrost abort

- **Classification:** INCORRECT / recovery-proof gap
- **Violated obligation:** `FIND-TASK-001-28` and its remediation acceptance
  criterion require in-process teardown to complete or abort existing
  Bifrost-owned work before runtime owners and `PgFixture` are released. The
  existing `Bifrost::abort` contract must be reused as the completion boundary.
- **Location:** `crates/wyrd/wyrd-testing/src/server.rs:831-847` and
  `:3675-3708`; insufficient closure proof at `:5479-5504`.
- **Evidence:** `settle_lifecycle` sets `bifrost_settled = true` before either
  operation completes. It gives `Bifrost::shutdown` a deadline and wraps it in
  the same `timeout_at`; after that deadline expires, it wraps
  `Bifrost::abort()` in that already-expired deadline. On timeout it only logs
  `"Bifrost storage did not settle"` and returns. `Drop` then sees the settled
  flag and does not retry. This contradicts `BifrostStorage::abort`, which is
  intentionally unbounded and returns only after all loaders and governed
  requests are idle (`crates/vala/vala-bifrost-redux/src/storage/mod.rs:
  1041-1058`). In-process mode reaches the abort path on every teardown because
  no `BoundServer::run` exists to mark Forge supervision drained.
- **Observable consequence:** an admitted storage operation that does not
  settle within the graceful budget can still be live when the harness drops
  its dedicated runtimes, storage root, and database fixture, recreating a
  scheduling-dependent teardown race while claiming quiescence.
- **Smallest testable correction:** retain the bounded graceful
  `Bifrost::shutdown`, but after any non-clean result await the existing
  `Bifrost::abort` to completion without reusing the elapsed grace deadline;
  set `bifrost_settled` only after clean shutdown or completed abort. Preserve
  the bound serve-task abort-and-join and production lifecycle APIs unchanged.
  Replace or extend the in-process test with the remediation-required live-work
  barrier so it proves that teardown cannot return and release the fixture
  before the abort owner has settled. The current test only observes an idle
  storage owner's before/after flag and exercises no live role or storage work,
  so it cannot close this path.

## Recovery and proof assessment

The recorded focused bound test credibly proves that a serve task which
outlives its drain is aborted and joined. The recorded in-process test proves
only that an idle storage owner becomes settled during ordinary off-runtime
drop. It does not install the required deterministic live-work barrier, does
not hold database- or storage-using work across teardown, and does not exercise
the warning/timeout branch at `server.rs:840-846`. The broader `test:wyrd` lane
therefore cannot establish the missing ordering merely by being green.

Recorded broader evidence is otherwise strong: format, lints, `test:skald`,
`test:wyrd`, docs, client-tier, and cumulative diff checks are reported green.
Those lanes do not replace the focused recovery-path proof required by the
remediation.

## Overall result

**FAIL**

`FIND-TASK-001-28` is not closed. The reported in-process Forge behavior is an
acceptable abort fallback because in-process mode owns no Forge supervisor,
but the harness can cancel that fallback and still mark the Bifrost owner
settled. `SYS-R5-001` is a bounded test-harness correction using the existing
abort owner; no specification or production lifecycle change is required.
