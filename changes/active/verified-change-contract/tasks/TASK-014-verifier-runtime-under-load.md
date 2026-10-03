---
id: TASK-014
kind: implementation
status: ready
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
