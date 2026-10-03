---
id: TASK-008-CLOSEOUT-R6
kind: remediation
status: implemented
spec: changes/active/verified-change-contract/spec.md
spec_revision: 57
original_task: changes/active/verified-change-contract/tasks/task-008-closeout.md
base: 345295d8e
reviewed_candidate: 54373c36856d41ffe3554c977c35e9d033918cdd
requirements: [REQ-171]
parent_task: TASK-008-CLOSEOUT
remediates: [FIND-TASK-008-CLOSEOUT-17]
route_to: wyrd-implement
---

# Close the final pending-to-durable audit handoff

## Outcome

Finish TASK-008 closeout by preventing the capacity drain from accepting zero
when an Oracle audit decision commits after the poll's first durable snapshot
and releases its process-local pending gauge before the final replica scrape.

This remediation applies to
`345295d8e..54373c36856d41ffe3554c977c35e9d033918cdd` under approved
specification revision 57. It preserves the accepted `pg_tests` placement,
the fixed-port nextest serialization, and all earlier accepted TASK-008
behavior. `FIND-TASK-008-CLOSEOUT-13` remains deferred to integration.

## Issue diagnosis

### `FIND-TASK-008-CLOSEOUT-17` — the final scrape is paired with a stale durable snapshot

REQ-171 requires every audit-outbox backlog caused by a capacity step to drain
within 60 seconds before the step passes. The candidate's `Queue::poll` now
observes:

1. every replica before the durable query (`S1`);
2. one PostgreSQL statement snapshot (`Q1`); and
3. every replica again (`S2`) only when `S1 + Q1` would otherwise be empty.

This closes the later-producer case while the decision remains pending through
`S2`. It does not close the successful pending-to-staging handoff after `Q1`.
Oracle's audit writer commits the canonical staging transaction before it
decrements `audit_outbox_pending`. A step-caused decision can therefore be
pending when `Q1` fixes a snapshot with no committed row, commit after that
snapshot, and decrement pending before `S2`. `Queue::poll` combines the zero
`S2` with the already captured zero `Q1` and returns empty while the committed
row remains above its tenant publication watermark. `Deployment::drain` can
then end the poll loop and falsely PASS the saturation cell.

The current public Oracle held-commit proof is the correct proof vehicle, as
directed by the integrator: the drain cannot distinguish which Oracle surface
created a pending decision. Its timing is incomplete. It holds the chain-head
lock until after `Queue::poll` returns, so the decision is necessarily still
pending at `S2`. It proves `S1 = 0, Q1 = 0, S2 = 1` and cannot fail under the
remaining `S1 = 0, Q1 = 0, commit/decrement, S2 = 0` schedule.

## Intended correction outcome

A terminating poll accepts zero only after its final process-local observation
is paired with a durable observation taken after that final scrape. Work that
is still pending at `S2` remains nonzero immediately; work that transferred to
staging across `Q1` is visible to the fresh durable observation. A genuinely
published or counted-lost decision may clear normally.

## Decision-complete recommendation

Keep the existing `Queue`, `Backlog`, `Queue::backlog`, first replica scrape,
first durable query, conditional second replica scrape, `Deployment::drain`,
and `Drain::judge` owners. Keep the Oracle pending gauge, canonical
`vala.audit_staging` append, non-blocking request behavior, `AuditPublisher`,
tenant watermark, and 60-second deadline unchanged.

At the existing `Queue::poll` decision boundary:

- retain the current early return when `S1 + Q1` is nonzero;
- retain the current nonzero return when `S2` reports replica-owned work; and
- only when `S2` is zero, call the existing `Queue::backlog` again after `S2`
  and combine that fresh durable value with `S2` before accepting zero.

The first durable zero establishes that no missing or nonterminal queued run
can become a new step-owned producer after `Q1`; the fresh durable read exists
only to observe a commit that crossed `Q1` before the zero `S2`. Do not add an
unbounded observation loop inside one poll. The outer drain loop already owns
repetition and the absolute deadline.

Reuse the current public Oracle held-commit harness. Arrange the same decision
after `S1`, let `Q1` observe the pre-commit state, then release the held commit
and wait for the pending gauge to fall before the terminating replica
observation completes. The poll must remain nonzero because its fresh durable
read sees the committed unpublished row. Continue through the existing
publisher path and prove zero only after the watermark advances.

This is the smallest correction at the consumer that decides whether all
owners are empty. It uses the existing process and durable evidence owners and
does not require a queued-Drift fixture, production audit change, distributed
snapshot, cross-system transaction, or blocking barrier.

## Constraints and preserved behavior

- Preserve the one `bench:capacity` entry point, revision-57 workload, step
  sequence, SLOs, report cells, verdict, profiling behavior, and absolute
  command deadline.
- Preserve the exact 60-second drain boundary and all run, Scribe, Forge,
  client-queue, CPU, and memory evidence.
- Preserve non-blocking authorization audit commits, counted loss, canonical
  staging, the single publisher, tenant-scoped watermarks, and publication
  deduplication.
- Preserve conservative over-counting during handoff; an extra outer poll is
  acceptable, but a false zero is not.
- Preserve the accepted `pg_tests`/ordinary-test split and ignored environment
  gate.
- Preserve the exact one-thread nextest group for the two fixed-port capacity
  tests.
- Preserve all process cleanup, workload, report, and public-contract behavior
  outside the corrected terminating poll.
- Keep FIND-13 deferred; do not claim AC-040/AC-041 empirical qualification.

## Non-goals

- No public API, CLI option, schema, storage format, audit table, publisher
  protocol, queue, timeout, SLO, report column, or configuration change.
- No blocking audit commit, request wait, distributed barrier, metrics/SQL
  transaction, new durable owner, or production audit redesign.
- No queued-Drift-specific test harness; use the approved public Oracle
  producer and existing held-commit harness.
- No change to test placement, fixed-port scheduling, replica cleanup, or
  earlier accepted TASK-008 behavior.
- No full default capacity qualification in this remediation.

## Acceptance criteria

### AC-R6-1 — a terminating poll closes the post-snapshot handoff

When `S1 + Q1` is empty, a decision still pending at `S2` keeps the returned
audit backlog nonzero. When that decision instead commits after `Q1` and
releases pending before `S2`, a fresh durable observation taken after `S2`
keeps the returned audit backlog nonzero until publication or counted loss.
The poll accepts zero only when the final replica and durable observations are
both empty. Closes `FIND-TASK-008-CLOSEOUT-17`.

### AC-R6-2 — the public Oracle proof exercises both sides of the handoff

The repository-managed Postgres proof retains the existing case in which the
decision remains pending through `S2`, and adds the terminating schedule in
which `Q1` reads before commit, the held commit completes, and pending reaches
zero before `S2`. In the latter schedule the poll remains nonzero from the
fresh durable row, stays nonzero while that row is above `published_seq`, and
reaches zero only after publication advances the watermark.

### AC-R6-3 — adjacent evidence and scheduling remain unchanged

The exact 60-second drain-edge proof, pure pending-plus-staged arithmetic,
complete capacity target, environment-test placement, fixed-port one-thread
group, report schema, and relevant server/audit journey retain their accepted
behavior. No public or durable contract is added.

## Focused proof and broader verification

Run the public Oracle handoff proof through the repository Postgres wrapper
and migrations with its exact `pg_tests` selector. Its assertions must prove:

- decision creation after `S1`;
- `Q1` observing no committed staging row;
- commit completion and pending decrement before the terminating replica
  observation;
- refusal to accept zero because the fresh durable read sees the row;
- continued nonzero backlog above the publication watermark; and
- zero only after publication advances that watermark.

Retain and run the still-pending phase in the same harness. Run the pure
pending-plus-staged arithmetic test and exact 60-second drain-edge test with
exact selectors. Then run the complete ignored-inclusive capacity target, the
focused fixed-port pair under nextest's default profile, the owning
server/audit journey, the focused `release_server` selection, `mise run fmt`,
`mise run lints`, and `git diff --check`. Use the repository-managed setup
wrapper for Postgres/live-server tests. Do not substitute the deferred full
`mise run bench:capacity` qualification for the focused handoff proof.

## Implementation evidence

Integrator direction: `Queue::poll` always takes the empty reading again as
scrape then durable read (`S1 -> Q1 -> S2 -> Q2`), accepts zero only when all
four are empty, and otherwise returns the latest combined value. The
completeness argument is in its rustdoc. Drain loop and judge unchanged.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-R6-1 terminating poll closes the post-snapshot handoff | `Queue::poll` (`capacity/evidence.rs`): when `S1 + Q1` is empty it scrapes again and calls `Queue::backlog` after that scrape, combining `Q2` with `S2` | Focused proof below; red check: returning `Q1` combined with `S2` (no `Q2`) fails `the second durable read sees the commit that crossed the first`, `left: 0, right: 1` | PASS |
| AC-R6-2 public Oracle proof exercises both sides | Same `pg_tests` proof keeps its still-pending phase (`late.audit == 1`) and adds a crossed phase: chain head held; read issued after `S1` (asserted pending 0 in `S1`); `Q1` runs with the writer blocked; inside `S2` a durable read asserts audit 0, the commit is released, pending is awaited to 0 and asserted 0 in `S2`; poll returns audit 1, a later drain read stays 1, publication makes it 0 | `evidence::pg_tests::the_audit_backlog_holds_from_a_pending_decision_until_its_publication` passes | PASS |
| AC-R6-3 adjacent evidence unchanged | Only `capacity/evidence.rs` changed (poll + proof) | Pure arithmetic and drain-edge tests 2/2; full capacity target with ignored 20/20; fixed-port pair 2/2 (serialized); `release_server::tests` 2/2; `test:bifrost:journey:server` 29/29 | PASS |

Non-goals held: no public, durable, audit, publisher, timeout, SLO, report,
test-placement, or nextest change. `FIND-TASK-008-CLOSEOUT-13` stays deferred;
default `bench:capacity` not run; no AC-040/AC-041 qualification claimed.

### Commands

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && \
  mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity --run-ignored=only \
  -E 'test(=evidence::pg_tests::the_audit_backlog_holds_from_a_pending_decision_until_its_publication)'"  # 1 passed; red without Q2: 1 failed
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity \
  -E 'test(=evidence::tests::pending_decisions_add_to_staged_audit_rows) | test(=step::tests::a_backlog_drains_only_within_the_limit)'  # 2 passed
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && \
  mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity --run-ignored=all"    # 20 passed
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity --run-ignored=all \
  -E 'test(=tests::a_stalled_tenant_setup_stops_the_run_by_its_deadline) | test(=tests::a_slow_replica_stop_leaves_the_runtime_free)'  # 2 passed
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(/^release_server::tests::/)'  # 2 passed
mise run test:bifrost:journey:server                                                           # 29 passed
mise run fmt && mise run lints && git diff --check                                             # clean
```
