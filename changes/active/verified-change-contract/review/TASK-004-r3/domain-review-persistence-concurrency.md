# TASK-004 R3 Persistence and Concurrency Domain Review

## Immutable Subject

- Base: `9431906eeb1c7b67a0efcec09487fd1d848f70a8`
- Candidate: `29721b7e33854633b025b25948fd5d2eaebe7bfd`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-004-generic-verification-runtime-and-results.md`
- Prior remediation: `changes/active/verified-change-contract/review/TASK-004-r1/TASK-004-R1-close-validated-runtime-gaps.md` and `changes/active/verified-change-contract/review/TASK-004-r2/TASK-004-R2-close-scheduler-ordering-and-source-shape.md`

`.codegraph/` is absent, so source and caller tracing used direct repository
inspection. This review covers the complete base-to-candidate range. The
candidate's deletion of uncalled `VerifierRunQueue::new` and
`ResultPublisher::endpoint` is owner-directed dead-code removal and changes no
reachable persistence or concurrency behavior.

## Reviewed Boundary

Postgres durable run and dispatch control state; scheduler occurrence locking,
atomic cursor advancement, and cancellation ordering; runner permit-before-claim
admission, leases, token fencing, retries, terminal settlement, cancellation,
drain, reclaim, and restart; runtime supervision and health; and the real
Postgres evidence for those paths.

## Authority and Source Coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Durable control-plane ownership | `REQ-078`, `INV-010`; `AGENTS.md` §§3, 9; `architecture/agent-rules.md` tenant-connection rules | `wyrd-sql` migrations `20260601000029_verifier_runs.sql` and `20260601000031_verifier_run_idempotency.sql`; `queries/verifier_runs.rs`; RLS and tenant/operator connection split | PASS |
| Exact frozen work and lifecycle | `REQ-079`, `REQ-097`, `INV-004`, `INV-011`; TASK-004 Scenarios 2, 4, 5 | Run schema constraints; shared enqueue; claim, complete, retry, terminate, release; dispatch insertion in the fenced completion transaction | PASS |
| Scheduled occurrence correctness | `REQ-081`, `AC-013`; TASK-004 Scenario 3 | `VerificationScheduler`; `VerifierRunQueue::schedule_next_due`; unique scheduled-occurrence index; concurrent scheduler and restart tests | PASS |
| Claims, leases, retries, and fencing | `REQ-079`, `REQ-115`, `AC-015`, `AC-020`; TASK-004 Scenario 4 | `VerifierRunner::{claim_round,claim,process,settle}`; token-fenced SQL; lease reclaim, stale settlement, exhaustion, release/refund, and restart tests | PASS |
| Concurrency limits and fairness | `REQ-146`, runtime portion of `AC-030`; TASK-004 Scenario 7 | `VerifierPermits`; permit-before-claim path; per-tenant/global saturation integration test | PASS |
| Cancellation, drain, and restart | `REQ-115`, `REQ-146`, runtime portion of `AC-030`; both remediation tasks | Scheduler pre-commit race and selected-commit linearization; runner pre-commit rollback and late-commit fenced release; 30-second production drain composition; capability supervision and health | PASS |

## Prior Finding Closure

### `FIND-TASK-004-5` — CLOSED

`VerificationScheduler::run` no longer races cancellation against the complete
pass. `schedule_tenant` races cancellation only through transaction acquisition
and the atomic run/cursor work (`scheduler.rs:163-177`). Cancellation before
that branch resolves drops the `TenantConn` and rolls the occurrence back. Once
the branch returns a transaction and tick, `conn.commit().await` is selected as
the occurrence's linearization point and is awaited without cancellation
(`scheduler.rs:178-188`). The post-commit cancellation check stops the same
tenant before another occurrence, and `pass` checks cancellation before every
later tenant (`scheduler.rs:129-141`). Thus shutdown observes one known result:
rollback before selection or one atomic run/cursor commit admitted before
closure.

The focused deferred-constraint-trigger test blocks the selected `COMMIT`,
cancels the runtime, proves the scheduler remains open, then releases the
commit and verifies exactly one run, one matching cursor advance, no later
occurrence, and stable restart state
(`pg_verification_runtime.rs:1252-1403`). The retained pre-commit scheduler
rollback, runner claim rollback, and runner late-commit release proofs cover
the other admission branches (`pg_verification_runtime.rs:1181-1250,1405-1522`).

The correction reuses the existing transaction, cancellation token, unique
occurrence fence, and runner release transition. It adds no shutdown table,
process-local admission registry, compensating write, or second claim protocol.

## Material Findings

None.

No reachable persistence, lease/fencing, retry, scheduler-ordering,
concurrency, drain, or restart defect remains in the reviewed TASK-004 boundary.

## Verification Evidence and Limits

- Independently reran
  `scheduler_awaits_its_selected_commit_before_closing`; it passed against the
  repository-managed Postgres instance.
- Independently reran
  `claim_committed_after_cancellation_is_released_unexecuted`; it passed against
  the repository-managed Postgres instance.
- A first combined four-test invocation ran the two pre-commit cancellation
  tests successfully, but its other two cases could not initialize isolated
  test servers because the shared Postgres environment was concurrently
  disrupted (`SSLRequest`/pool startup errors). Both affected behaviors have
  recorded green candidate evidence, and the two individually rerun
  commit-race cases passed; this infrastructure collision is a verification
  limit, not evidence of a product failure.
- The complete broad recorded evidence in the R2 implementation record reports
  passing `test:sql`, `test:wyrd`, `test:vala`, both Bifrost server lanes,
  tenant/pool/client/unwrap boundaries, formatting, lints, and diff checks. This
  reviewer did not rerun that full matrix.
- Operator-worker delivery execution remains assigned to a later task; this
  review covers TASK-004's durable dispatch handoff and status, not an
  unimplemented later worker engine.

## Overall Result

**PASS**

The cumulative candidate satisfies the reviewed persistence and concurrency
obligations. `FIND-TASK-004-5` is closed with both a source-level linearization
argument and a real deferred-commit proof, while the previously accepted
durable queue, fencing, retry, capacity, drain, and restart behavior remains
intact.
