---
id: TASK-003-R1
kind: remediation
status: review
spec: SPEC-forge-concurrent-planning
spec_revision: 11
parent_task: TASK-003
remediates: [FIND-TASK-003-1]
---

# Preserve refused prepared-cleanup ownership

## Contract and candidates

- Approved spec: `changes/active/forge-concurrent-planning/spec.md`
- Original task: `changes/active/forge-concurrent-planning/tasks/TASK-003-maintenance-and-removal.md`
- Reviewed candidate/base: `7ac45dec99535c881b7a936c66623044f15d8823` / `c1508b375`

## Diagnosis

A fresh active read or refreshed root can safely refuse a prepared cleanup.
Settlement intentionally leaves the row `prepared`, but the worker then routes
the refusal through generic failure settlement. `retry_failure` accepts only
`claimed` or `running`, so it reports lost attempt ownership and abandons the
immediate same-identity replay path.

## Intended correction outcome

Refusal or uncertain deletion retains the exact prepared identity and returns
to existing reconciliation without a slot-fatal ownership error; once the root
clears, that identity converges.

## Decision-complete recommendation

Represent the already-existing retained-prepared outcome explicitly at the
worker settlement boundary. Bypass generic retry/terminal transitions for that
outcome and route it to the existing prepared-candidate reconciliation owner
under the same task/attempt identity. Add no durable state, retry ledger, or
parallel cleanup owner.

## Preserved behavior and non-goals

- Preserve fail-closed refusal, active-read/root checks, uncertain-effect
  evidence, idempotent deletion, and normal retry behavior for claimed/running
  failures.
- Do not weaken deletion proof or synthesize success.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-003-1` | A prepared candidate refused by a newly visible root produces no settlement conflict and replays the same identity after the root clears. |

## Focused proof and broader verification

Prepare a cleanup candidate, add an active read before delete proof, assert
refusal with retained identity and no ownership error, release the reader, and
prove convergence. Run the cleanup integration/journey lanes, owning Bifrost
verification, format, lints, and diff check.


## Implementation evidence

Base `e8d3cca13`. The drain's existing retained-prepared outcome is now the
typed `ForgeError::CleanupRetained { index, transition }`
(`crates/vala/vala-bifrost-redux/src/forge/error.rs`), returned by
`drain_expired_cleanup` when settlement deliberately left a refused or
uncertain candidate `prepared`. `settle_execution_failure` treats it like
`ShutdownRetained` (commits nothing, bypasses retry/terminal/dispatched close).
On the prepared-claim route, `reconciliation_is_fatal` exempts it from
`close_after_fatal`, and `reconcile_one_prepared` returns `Ok(false)` so the
slot loop paces the same-identity replay. No durable state, SQL, or new owner
was added.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-003-1`: a prepared candidate refused by a newly visible root produces no settlement conflict and replays the same identity after the root clears | `forge/error.rs` `CleanupRetained`; `forge/worker.rs` `drain_expired_cleanup`, `settle_execution_failure`, `reconciliation_is_fatal`, `reconcile_one_prepared`, `reconcile_prepared`, `reconcile_prepared_attempt` | `forge::expired_cleanup::refused_prepared_cleanup_retains_identity_and_replays_after_root_clears`: fair-claimed refusal settles `Ok` (trace shows `retained for same-identity replay`, no `lost attempt ownership`); active read taken after preparation refuses the same owner's replay as `CleanupRetained { index: 0 }` with cursor `prepared/0/Some(0)`, same attempt, zero deletes; after release the same identity succeeds and removes every candidate | PASS |
| Preserved fail-closed refusal, uncertainty, idempotent deletion, takeover | Unchanged proof/settlement owners | `cursor_replays_exact_prepared_candidate_after_refusal_uncertainty_and_takeover` (assertions retyped to `CleanupRetained` with exact indices); `terminal_file_list_row_is_removed_only_after_object_cleanup` (fair-claimed uncertain delete now settles `Ok` and retains its prepared candidate) | PASS |

Commands (all green):

- `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E "test(=forge::expired_cleanup::refused_prepared_cleanup_retains_identity_and_replays_after_root_clears) | test(=forge::expired_cleanup::cursor_replays_exact_prepared_candidate_after_refusal_uncertainty_and_takeover) | test(=forge::expired_cleanup::terminal_file_list_row_is_removed_only_after_object_cleanup)"'` — 3/3 passed
- `mise run test:bifrost:integration:redux` — 891/891 passed
- `mise run test:bifrost:journey:forge` — 21/21 passed
- `mise run fmt`, `mise run lints`, `git diff --check` — clean

Diagnosis recorded during iteration: the first focused run failed two new
assertions expecting `execute_one_for_test` to return `false`. Cause:
`execute_one_for_test` returns `true` whenever it executed a claim (it discards
`execute_claim`'s effect flag); the trace showed the refusal settled as
retained with no ownership error. Fix site: the test assertions only.

Non-goals held: no SQL, durable state, retry ledger, or parallel cleanup owner;
deletion proof and refusal checks unchanged. Files changed: `forge/error.rs`,
`forge/worker.rs`, `tests/integration/forge/expired_cleanup.rs`, this task.

### Follow-up: active read refusing a candidate *preparation*

The same root pattern also hit candidate preparation: an active read made
`prepare_expired_cleanup_candidate` return an untyped `SqlError::Conflict`,
which reached generic retry. For a row already `prepared` that reported lost
ownership; for the first preparation it consumed retry budget.

Fix at the SQL owner: `prepare_expired_cleanup_candidate` now returns
`ExpiredCleanupPreparation` (`crates/vala/vala-sql/src/row_types/forge_tasks.rs`),
whose `ActiveReadRefused` variant rolls back and writes nothing. In the worker,
`ForgeWorker::preparation_refused` maps it from the durable row it implies:
index 0 (row still `running`, no evidence) becomes `ForgeError::Capacity`,
which releases the claim without consuming retry budget. Any later index (row
already `prepared`) becomes `ForgeError::CleanupRetained`, which the
prepared-claim route replays. Neither outcome reaches generic retry.

| Criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Typed SQL refusal, no write | `queries/forge_tasks.rs` `prepare_expired_cleanup_candidate`; `row_types/forge_tasks.rs` `ExpiredCleanupPreparation` | `pg_tests::expired_cleanup_handoff_and_candidate_lifecycle_are_exact_atomic_and_audited` asserts `Ok(ActiveReadRefused)` and no evidence | PASS |
| First-preparation refusal settles without generic retry | `worker.rs` `drain_expired_cleanup`, `preparation_refused` | `forge::expired_cleanup::active_read_refusing_preparation_is_released_or_retained_without_retry`: `retryable / capacity_refused / attempt_count 0`. Red before the fix: `transient_coordination`, attempt_count 1 | PASS |
| Later-preparation refusal is retained, replays, and converges | same | same test: cursor `prepared/1/None` with no ownership error; the held-read replay returns `CleanupRetained { index: 1 }` with the same attempt and one delete total; after release the task succeeds and every candidate is removed | PASS |

Commands:

- `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-sql --test pg_forge_tasks -E "test(=pg_tests::expired_cleanup_handoff_and_candidate_lifecycle_are_exact_atomic_and_audited)"'`: 1/1 passed
- The same wrapper with `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all` and an `-E` filter naming the four `forge::expired_cleanup::` tests above: 4/4 passed
- `mise exec -- cargo nextest run --locked -p vala-sql --test pg_forge_tasks` (whole target, under the Postgres wrapper): 24/24 passed
- `mise run test:bifrost:integration:redux`: 892/892 passed
- `mise run test:bifrost:journey:forge`: 21/21 passed
- `mise run fmt`, `mise run lints`, `git diff --check`: clean

Non-goals held: no migration, durable state, or new owner. `refuse_active_table_reads`
still serves its other caller (`forge_operations.rs`) unchanged.
