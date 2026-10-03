---
id: TASK-014
kind: implementation
status: review
spec: SPEC-verified-change-contract
spec_revision: 59
requirements: [REQ-087, REQ-181, REQ-182, REQ-183, REQ-184, REQ-185, REQ-114, INV-021, AC-044]
depends_on: [TASK-013]
---

# Run Verifiers under load: one stored result, renewed leases, short connections

## Outcome and Value

A run's result is decided once and stored in Postgres under its lease before
any of it reaches Bifrost; every later write, by any claimant, replays the
same bytes and batch IDs, and the settle that completes the run deletes it
(REQ-087, REQ-183, INV-021). Claim rounds visit every tenant with a claimable
run (REQ-181). The claim returns the Verifier Card status, and its spec only
on a per-process cache miss, so loading a Verifier opens no connection
(REQ-182). Leases are renewed per tenant in one statement and a lost lease
cancels its work (REQ-184). A run holds a connection only for its claim,
store, settle, and the shared renewal; connection unavailability and a full
shared resource never consume an attempt, and claiming pauses until a running
run finishes (REQ-181, REQ-185). AC-044 proves all of it under load.

## Owners, Scope, Consumers, and Prohibited Changes

- `wyrd-sql` owns the staged-result table (migration, RLS, `TenantConn`
  access), the fenced store, the renewal statement, the claim's Card status
  and spec, and the unlimited tenant lists.
- `wyrd-server` `verification/` owns the runner, claim loop, cache, renewal
  task, and refusal pause.
- Prohibited: an execution count cap or reservation, a configurable cache
  size, cross-tenant cache sharing, the withdrawn revision-58 half-pool bound,
  a token in any phase, re-executing a run with a stored result, rebuilding a
  stored result.

## Approach

1. Add the staged-result table and fenced store/read/delete statements; the
   complete statement deletes it; an expired lease with a stored result is not
   exhausted.
2. Return Card status, SYSTEM principal, staged-result presence, and (on a
   miss) the Card spec and the fitted Drift baseline from the claim
   transaction; add a 64 MiB byte-bounded LRU per process.
3. Store before writing; replay a stored result on any claim that finds one.
4. Renew every in-flight lease of a tenant in one statement once a third of
   the lease has passed; cancel the run whose token is gone.
5. Remove the round tenant limit; treat pool timeouts and admission/backpressure
   refusals as shared-resource refusals that release without an attempt and
   pause claiming until a running run finishes; retry store and settle with
   backoff while the lease holds.
6. Update architecture for the runtime.

## Ordered Implementation Scenarios

### Scenario 1 — A stored result is fenced, replayed, and deleted at settle

**Behavior.** Only the lease holder stores; a stale token stores nothing; the
claim reports a stored result; completing deletes it (REQ-183).

**RED.** `pg_verifier_runs` test of store with current and stale tokens,
reclaim visibility, and deletion at completion.
`scripts/postgres/with-test-postgres.sh mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_verifier_runs -E 'test(=stored_results_are_lease_fenced_and_deleted_at_settle)'`

**GREEN.** Migration plus queue methods.

**REFACTOR.** Fold the staged read into the claim transaction.

### Scenario 2 — Every tenant is claimed in the first round

**Behavior.** With more than 64 tenants each holding a claimable run, one round
claims all (REQ-181, AC-044).

**RED.** Runtime test with 70 tenants.
`mise run test:bifrost:integration:server` selecting
`every_tenant_is_claimed_in_the_first_round` with its exact nextest command.

**GREEN.** Delete the limits.

**REFACTOR.** Remove the limit parameter from the claim loop.

### Scenario 3 — Verifier cache and deleted Verifier

**Behavior.** The second run of one Verifier on a process reads no Card spec;
a deleted Verifier settles `errored` (REQ-182, AC-044).

**RED.** Runtime test counting spec loads and deleting a Verifier.

**GREEN.** Claim-returned status/spec and the LRU.

**REFACTOR.** Delete `load_verifier`'s own connection.

### Scenario 4 — Reclaimed stored result is written without re-execution

**Behavior.** A run whose lease is reclaimed after storing writes the stored
result; each table holds one copy; a stale claimant's work never reaches
Bifrost (REQ-087, REQ-183, AC-044).

**RED.** Runtime tests with a scripted crash after store and a stolen lease.

**GREEN.** Store-then-write ordering and replay on claim.

**REFACTOR.** One write path for fresh and stored results.

### Scenario 5 — Lease renewal and cancellation

**Behavior.** A run executing longer than its lease keeps it; a renewal that
finds its token gone cancels the work (REQ-184, AC-044).

**RED.** Runtime test with a short lease and a held run, then a stolen token.

**GREEN.** Per-tenant renewal statement and task.

**REFACTOR.** Renewal shares the claim loop's tenant bookkeeping.

### Scenario 6 — 200 held runs and shared-resource refusal

**Behavior.** 200 held runs on the 8-connection pool all complete on their
first attempt with no `settlement_failed` and no acquire timeout; a refused run
returns without consuming an attempt and claiming resumes when a run finishes
(REQ-181, REQ-185, AC-044).

**RED.** Runtime tests for both.

**GREEN.** Connection discipline, store/settle retry within the lease, and the
refusal pause.

**REFACTOR.** Remove the publication timeout.

## Acceptance Criteria

Every AC-044 bullet passes; existing runtime, Drift, and Eval journeys stay
green; `scripts/check_tenant_isolation.py` accepts the new table.

## Expected Write Set and Consumer Closure

`crates/wyrd/wyrd-sql/{migrations,src/queries/verifier_runs.rs,tests/pg_verifier_runs.rs}`,
`crates/wyrd/wyrd-server/src/verification/*`,
`crates/wyrd/wyrd-server/tests/pg_verification_runtime.rs`,
`architecture/wyrd-design.md`.

## Verification and Evidence

Exact focused commands for each named test; `mise run fmt`, `mise run lints`,
`mise run test:sql`, `mise run test:bifrost:integration:server`,
`mise run test:bifrost:integration`, `mise run check:tenant-isolation` when
present.

## Material Stop Conditions

- Holding the lease while a stored result cannot be written conflicts with
  shutdown or drain semantics.
- A shared resource cannot report a capacity refusal distinctly from failure.

## Authority Links

- [Approved spec revision 59](../spec.md): REQ-087, REQ-181..185, INV-021,
  AC-044.
- `AGENTS.md`, `architecture/bifrost-design.md` (bounded buffers).

## Implementation Evidence

### Acceptance matrix (AC-044)

| AC-044 bullet | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| 200 released runs complete on their first attempt on the 8-connection pool, no `settlement_failed`, no acquire timeout | Connections only for claim, store, settle, renew (`runner.rs`); `persist` retries store/settle with backoff while the lease holds | `pg_verification_runtime::two_hundred_released_runs_complete_on_their_first_attempt` | PASS |
| More than 64 tenants all claimed in the first round | `TENANTS_PER_ROUND` and the due-tenant `LIMIT` removed; a failing tenant claim is logged and skipped (`claims.rs::claim_round`) | `pg_verification_runtime::every_tenant_is_claimed_in_the_first_round` (70 tenants) | PASS |
| Reclaim after a stored result replays it without re-executing; one copy per result table | `wyrd.verifier_run_results` (migration `20261003000000`), `store_result`, claim returns `staged`, deleted at settle | `pg_verification_runtime::crash_after_detail_ack_reclaims_the_same_run_before_dispatch`; `pg_verifier_runs::stored_results_are_lease_fenced_and_deleted_at_settle` | PASS |
| A stale claimant cannot store and never reaches Bifrost | Store is lease-fenced; `LeaseLost` settles nothing and writes nothing | `pg_verification_runtime::expired_lease_is_reclaimed_and_the_stale_holder_is_fenced`; `renewal_keeps_a_long_run_and_a_taken_token_cancels_it` | PASS |
| Long run keeps its lease via renewal; a missing token cancels the work | `verification/leases.rs` `LeaseRenewal` (per-tenant statement after a third of the lease); `RENEW_LEASES_SQL` never revives an expired lease | `pg_verification_runtime::renewal_keeps_a_long_run_and_a_taken_token_cancels_it`; `pg_verifier_runs::leases_renew_once_a_third_has_passed_and_never_revive` | PASS |
| A second run of one Verifier opens no Card connection; a deleted Verifier settles `errored` | Claim returns Card status (spec only on cache miss); `verification/cache.rs` 64 MiB per-process LRU keyed by (tenant, uid) | `pg_verification_runtime::verifier_cards_are_cached_and_a_deleted_verifier_errors`; `verification::cache::tests::entries_are_tenant_scoped_and_evicted_least_recently_used_by_bytes` | PASS |
| A shared-resource refusal returns the run without an attempt; claiming resumes when a running run finishes | `Transition::Defer` releases without an attempt; the claim loop pauses until a spawned run finishes | `pg_verification_runtime::a_refused_run_pauses_claiming_until_a_running_run_finishes` | PASS |

### Test changes with reasons

- `pg_verification_runtime::unacknowledged_summary_retries_with_a_fresh_result`
  was deleted. REQ-183 and INV-021 require every attempt to replay the stored
  result, so a fresh result can no longer happen.
- `expired_lease_...` and `crash_after_detail_ack_...` now assert that the
  stored result is replayed: one engine entry and identical feature rows.
- `eval_verification::continuous_eval_failures_publish_only_stable_errors`:
  the claim now reads the Verifier Card, so a Card read that raises rolls the
  claim back. The test now asserts that no run is claimed and no attempt is
  used while the policy stands. After the policy is dropped, the runs settle
  `eval_execution_failed` against the failing provider. The no-leak
  assertions are unchanged.
- `RuntimeLimits::publication_timeout` was removed, along with its use in
  `wyrd-testing`.

### Commands

```
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::cache::tests::entries_are_tenant_scoped_and_evicted_least_recently_used_by_bytes)'
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_verification_runtime --test-threads=1 -E 'test(=every_tenant_is_claimed_in_the_first_round) | test(=verifier_cards_are_cached_and_a_deleted_verifier_errors) | test(=renewal_keeps_a_long_run_and_a_taken_token_cancels_it) | test(=two_hundred_released_runs_complete_on_their_first_attempt) | test(=a_refused_run_pauses_claiming_until_a_running_run_finishes) | test(=crash_after_detail_ack_reclaims_the_same_run_before_dispatch) | test(=expired_lease_is_reclaimed_and_the_stale_holder_is_fenced)' && mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_verifier_runs -E 'test(=leases_renew_once_a_third_has_passed_and_never_revive) | test(=stored_results_are_lease_fenced_and_deleted_at_settle)'"
mise run fmt; mise run lints; mise run check:tenant-isolation
mise run test:sql; mise run test:bifrost:integration:server; mise run test:operators:integration
mise run test:bifrost:journey:server
```

The last command ran 29 tests: 27 passed and 2 failed. The failures are
`continuous_eval_runs_the_terminal_matrix` and
`sealed_replay_on_a_later_day_activates_once`. Both come from the parked
TASK-015 WIP outbox (commit 16ba1d2c8), not from this task. That outbox has a
single writer and retries the whole batch, and it is being replaced by the
shared generic outbox. There is no `test:bifrost:integration` aggregate task,
so the `server` variant was run instead.

### Residual risks

- Eval judge Card reads still open a short connection while the run executes.
- The fitted baseline comes back on every claim, not only on a cache miss.
- A Card read that keeps failing in SQL now blocks claims for that tenant
  indefinitely, without using attempts. It is logged once per round.
