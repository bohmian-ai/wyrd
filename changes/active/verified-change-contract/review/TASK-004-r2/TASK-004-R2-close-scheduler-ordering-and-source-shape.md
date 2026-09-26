---
id: TASK-004-R2
kind: remediation
status: proposed
spec: SPEC-verified-change-contract
spec_revision: 33
parent_task: TASK-004
remediates: [FIND-TASK-004-5, FIND-TASK-004-8]
---

# Close scheduler ordering and cumulative Rust source shape

## Authority and subject

- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-004-generic-verification-runtime-and-results.md`
- Reviewed base: `9431906eeb1c7b67a0efcec09487fd1d848f70a8`
- Reviewed candidate: `2af4cc3ff95a609d1df682be4f633345f96934e1`
- Validated ledger: `changes/active/verified-change-contract/review/TASK-004-r2/findings-validation.md`

## Outcome

Close the remaining shutdown-ordering defect so every scheduler occurrence has
a known durable outcome at cancellation, and finish the required top-level
import/bare-type cleanup across the cumulative changed Rust surface. Preserve
all accepted TASK-004 behavior and architecture.

## Issue diagnosis and required correction

### `FIND-TASK-004-5` — scheduler shutdown can discard an unknown commit outcome

`VerificationScheduler::run` races cancellation against the whole pass future.
`schedule_tenant` owns `TenantConn::commit().await` inside that future. If stop
arrives after SQLx sends `COMMIT` but before it receives PostgreSQL's response,
the select drops the only future that can report whether the atomic run/cursor
transaction committed. The implementation itself concedes that the commit may
land after stop. The current lock-controlled scheduler test blocks before
`COMMIT`, so it proves rollback of uncommitted work but not this reachable
branch. Shutdown may therefore expose a new pending run and advanced cursor
without a known admission ordering, falling short of `REQ-146`, `AC-030`,
TASK-004 Scenario 7, and the prior remediation's closure criterion.

Keep the existing scheduler owner, cancellation token, tenant transaction, and
atomic run/cursor write. Establish one linearization point before commit. If
cancellation wins before commit selection, roll back and start no later
occurrence or tenant. Once commit is selected, await it to a known result
without cancellation; classify a successful transaction as admitted before
scheduler closure, then exit if cancellation arrived. This closes the unknown
outcome without compensating durable state.

Do not add a shutdown table, process-local admission registry, compensating
delete or cursor rewrite, second scheduling protocol, recovery coordinator, or
new persistence contract.

### `FIND-TASK-004-8` — cumulative changed Rust still uses qualified signature types

The prior cleanup corrected its enumerated locations but did not inspect the
complete cumulative changed surface. Mandatory `architecture/agent-rules.md`
source shape is still violated at:

- `crates/wyrd/wyrd-server/src/app/server.rs:513-517`;
- `crates/wyrd/wyrd-server/src/state.rs:2088`;
- `crates/wyrd/wyrd-server/src/verification/publisher.rs:86-88`;
- `crates/wyrd-spec/src/ids.rs:264-267`; and
- `crates/vala/vala-bifrost-redux/src/gate/mod.rs:2493-2508`.

Add the cited types to each module's existing top-level import block and use
bare names in the affected fields, returns, parameters, and impl headers. Use
one narrow alias only if `std::fmt::Result` collides with ordinary `Result`.
Inspect the complete cumulative Rust diff for the same prohibited shape so a
third partial cleanup is unnecessary.

This correction changes spelling only. Do not reorganize modules, add a helper,
abstraction, dependency, lint, or permanent source check.

## Preserved behavior and non-goals

- Preserve schedule insert/cursor atomicity, unique occurrence identity,
  no-catch-up behavior, permit-before-run-claim ordering, lease fencing,
  retries, attempt accounting, and the existing 30-second drain.
- Preserve the runner's fenced late-claim release/refund path and all completed
  security, Gate/Scribe, result mapping, ACK, crash/reclaim, SDK/MCP, and
  role-separated journey behavior.
- Preserve the existing schemas, migrations, public APIs, permissions, audit
  cardinality, SYSTEM token contract, concurrency limits, and deployment
  shutdown budgets.
- Do not implement later Drift, Eval, or Operator engines, or claim atomic
  analytical visibility.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-004-5` | Cancellation before scheduler commit selection produces a known rollback and starts no later tenant; commit selection before cancellation is awaited to a known result, with exactly one atomic occurrence classified as admitted before closure; restart observes precisely that state. |
| `FIND-TASK-004-8` | Every newly added or materially modified Rust field, signature, bound, and impl header in the cumulative task diff uses top-level imported bare types; all cited locations are corrected without behavior change. |

## Focused and broader proof

Add one real-Postgres deferred-commit test using the existing transaction-lock
or fault machinery. Cancel while scheduler `COMMIT` is blocked, release it,
and prove the chosen ordering: either zero runs plus the original cursor when
cancellation wins before selection, or exactly one run plus its atomic cursor
advance when commit selection wins. Prove no next occurrence or tenant begins
after cancellation and restart preserves the observed state. Retain the
existing scheduler pre-commit rollback and runner late-commit release tests.

No behavior test is needed for the source-shape correction. Reinspect the
complete cumulative diff, then run the exact focused scheduler test command
after confirming its final name, followed by the affected verification:

```bash
mise run test:sql
mise run test:wyrd
mise run test:vala
mise run test:bifrost:integration:server
mise run test:bifrost:journey:server
mise run check:tenant-isolation
mise run check:from-pools-allowlist
mise run check:client-tier
mise run check:unwrap-audit
mise run fmt
mise run lints
git diff --check
```

Route this remediation directly to `$wyrd-implement`. The next task review
must inspect the complete original base-to-remediated-candidate range.

## Implementation evidence

Commits: `b2d80a663` (scheduler ordering), `531965b39` (source shape).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-004-5` | `crates/wyrd/wyrd-server/src/verification/scheduler.rs`: `run` no longer races the whole pass; `pass(&stop)` starts no tenant after cancellation; `schedule_tenant` races only connection open plus the atomic run/cursor apply against `stop` (drop = rollback), then awaits the selected `commit()` without cancellation, records the tick, and returns if `stop` fired. | New `pg_verification_runtime::scheduler_awaits_its_selected_commit_before_closing` (deferred constraint trigger blocks `COMMIT` on an advisory lock; cancel; scheduler stays open while the commit is in flight; release; exactly one run and one cursor advance; second due binding untouched; restart keeps the admitted run/cursor and schedules only the untouched binding). Retained `cancelled_scheduler_rolls_back_its_blocked_occurrence` and `claim_committed_after_cancellation_is_released_unexecuted` pass. | PASS |
| `FIND-TASK-004-8` | Cited locations in `app/server.rs`, `state.rs`, `verification/publisher.rs`, `wyrd-spec/src/ids.rs` (`FmtResult` alias: `Result` collides), `vala-bifrost-redux/src/gate/mod.rs`; full cumulative-diff scan from `9431906e` also corrected new `details: Value` fields in `wyrd-spec/src/error.rs` and the new `insert_as`/`SystemWriterHarness` signature types in `wyrd-server/tests/pg_grpc_ingest_smoke.rs`. Spelling only. | `mise run lints`, `mise run fmt`, full lanes below | PASS |

Commands run in this session, all exit 0:

- `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_verification_runtime -E "test(=scheduler_awaits_its_selected_commit_before_closing) | test(=cancelled_scheduler_rolls_back_its_blocked_occurrence) | test(=claim_committed_after_cancellation_is_released_unexecuted) | test(=schedulers_create_one_run_per_occurrence_across_ticks_and_restart)"'` (4 passed)
- `mise run test:sql`; `mise run test:wyrd` (2121 passed); `mise run test:vala` (1234 passed); `mise run test:bifrost:integration:server` (79 passed); `mise run test:bifrost:journey:server` (15 passed)
- `mise run check:tenant-isolation`; `mise run check:from-pools-allowlist`; `mise run check:client-tier`; `mise run check:unwrap-audit`; `mise run fmt`; `mise run lints`; `git diff --check`

Non-goals held: no schema, migration, public API, permission, audit, shutdown-budget, or drain change; no shutdown table, admission registry, compensating write, or new check. Existing `wyrd-spec` `serde_json::Value` fields outside this change were left untouched.
