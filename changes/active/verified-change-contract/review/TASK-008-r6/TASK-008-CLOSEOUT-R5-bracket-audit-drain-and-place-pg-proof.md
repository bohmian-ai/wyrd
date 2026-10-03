---
id: TASK-008-CLOSEOUT-R5
kind: remediation
status: ready
spec: changes/active/verified-change-contract/spec.md
spec_revision: 57
original_task: changes/active/verified-change-contract/tasks/task-008-closeout.md
base: 1d05642bf2c4d824de1aec27286048ec79b37e25
reviewed_candidate: f8d6945041467d52020024d311ce8831a45ff4a1
requirements: [REQ-171]
parent_task: TASK-008-CLOSEOUT
remediates: [FIND-TASK-008-CLOSEOUT-17, FIND-TASK-008-CLOSEOUT-18]
route_to: wyrd-implement
---

# Bracket the audit-drain snapshot and place its Postgres proof correctly

## Outcome

Finish TASK-008 closeout by preventing the capacity drain from accepting a
false zero when queued verifier execution creates audit work between its
replica and PostgreSQL observations, and place the real Postgres/live-server
proof at the repository's required test boundary.

This remediation applies to the cumulative candidate rooted at
`1d05642bf2c4d824de1aec27286048ec79b37e25` and reviewed at
`f8d6945041467d52020024d311ce8831a45ff4a1` under approved specification
revision 57. It preserves the accepted closure of
`FIND-TASK-008-CLOSEOUT-16`, the existing pending gauge, canonical audit
staging and publication path, report semantics, and caller-deferred default
capacity run.

## Issue diagnosis

### `FIND-TASK-008-CLOSEOUT-17` — the drain does not bracket a later audit producer

REQ-171 requires every audit-outbox backlog caused by a step to drain within
60 seconds before the step passes. The candidate correctly mirrors
`OracleQueryAudit`'s process-local pending owner into
`audit_outbox_pending`, keeps that owner nonzero until commit or counted loss,
and counts every staged row above the tenant publication watermark. Scraping
replicas before the durable query protects a decision that is already pending
and commits during that query.

That ordering does not cover a decision created after the scrape. A queued
Drift request returns after its run row is accepted, not after runner
execution. During drain, the runner can reach its ordinary audited Oracle
query after the replica scrape. Oracle stages the decision, then the query,
result publication, and run settlement can complete while the independent
audit writer remains delayed before its staging commit. The following SQL
snapshot can therefore see every accepted run created, the producer run
terminal, and no staged audit row. Combining that snapshot with the stale
pre-query zero gauge lets `Drain::judge` accept empty while the replica still
owns the audit decision.

The current held-chain-head proof creates both public Oracle decisions before
its first drain observation. It proves pending-before-scrape, late commit,
watermark retention, and publication-to-zero, but it cannot exercise a queued
verifier that first produces its audit decision between the observations.

### `FIND-TASK-008-CLOSEOUT-18` — the environment-owned proof is in the unit-test module

The candidate adds a proof that starts `WyrdTestServer`, uses fixture
PostgreSQL, holds a tenant audit-chain transaction, and drives
`AuditPublisher`. It and its environment-specific helpers live under the
ordinary `#[cfg(test)] mod tests` in `capacity/evidence.rs`.

Repository rules require tests needing Postgres, Docker, or a live server to
live in `mod pg_tests` or a `pg_*` file. The existing `#[ignore]` attribute is
the correct execution gate but does not provide the required source
classification. The consequence is that an environment-owned integration
proof is structurally presented as an ordinary credential-free unit test.

## Intended correction outcome

One drain poll cannot accept zero unless its durable snapshot is bracketed by
replica observations that cover both directions of the audit handoff:

- the pre-query observation covers work already pending that commits while
  PostgreSQL is read;
- durable staging covers committed, unpublished work; and
- a post-query observation covers work first created after the pre-query
  observation.

The real queued-verifier proof exercises that full interval from request
acceptance through Oracle audit ownership, terminal run settlement, durable
staging, and publication. Environment-owned proof code is discoverable under
the repository's required `pg_tests` boundary, while pure evidence arithmetic
remains in ordinary unit tests.

## Decision-complete recommendation

Keep `OracleQueryAudit::pending`, the `audit_outbox_pending` gauge, the
pre-query replica scrape, `Queue::backlog`, and the existing unpublished-row
query unchanged as the three existing evidence mechanisms. At the current
`Deployment::drain` decision boundary, only when the combined pre-query and
durable snapshot would otherwise be empty, scrape every serving replica again
and require the current pending-audit total to be zero before returning
`Drain::Drained`. If that post-query observation is nonzero, retain it in the
audit backlog and continue the existing poll loop. Use that post-query scrape
as the final returned replica evidence for a zero decision so the record
matches the observation that permitted the drain to finish.

This is the minimum correction because the pre-query observation must remain
to cover pending-to-staging movement during SQL, and the durable query must
remain to cover publication ownership after commit. A second observation only
at the otherwise-empty decision point covers the later producer without
adding a barrier, request wait, transaction, queue, ledger, endpoint,
configuration, public contract, or new lifecycle owner. Do not serialize
queued execution with the audit writer and do not make audit commits blocking.

Extend the real proof through the queued Drift path rather than adding a
parser-only approximation. Arrange for its Oracle audit decision to become
pending after the first replica observation, hold the audit-chain commit,
allow the verifier run to settle terminally, and demonstrate that the
intervening SQL snapshot cannot make the drain accept zero. Then release the
commit and prove the existing durable audit cell remains nonzero until the
publication watermark advances.

Within `capacity/evidence.rs`, reuse an in-source `#[cfg(test)] mod pg_tests`
for the Postgres/live-server helpers and real handoff proof. Keep pure
`Backlog`/metrics arithmetic and percentile tests in the ordinary test module.
Retain the environment gate and update the exact focused selector to the new
module path. Do not create another integration-test binary, duplicate the
helpers, or broaden the fast lane.

## Constraints and preserved behavior

- Preserve the one `bench:capacity` entry point, four-tenant workload,
  direct/queued five-kind mix, Scribe ingest and Oracle query rates,
  warmup/ramp/knee/sustained/scale-out sequence, SLOs, report cells, verdict,
  profiling behavior, and one absolute command deadline.
- Preserve the exact 60-second drain edge and the existing run, Scribe, Forge,
  client-queue, CPU, and memory evidence.
- Preserve non-blocking authorization decisions, the canonical
  `vala.audit_staging` append, the single `AuditPublisher`, tenant-scoped
  watermarks, counted audit loss, and request latency semantics.
- Preserve conservative over-counting across pending-to-staging handoff; a
  poll may repeat but must not accept a false zero.
- Preserve `Deployment` ownership of replica-set shutdown, `spawn_blocking`,
  newest-first stop, ordinal result/log mapping, grace, kill/reap, and
  cancellation behavior.
- Preserve the existing environment gate for the live-server proof and the
  credential-free default capacity test selection.
- Keep `FIND-TASK-008-CLOSEOUT-13` deferred to integration. This remediation
  does not run or claim the full default performance qualification.
- Treat the intermittent
  `verification_runtime::two_bindings_share_one_client_observation` failure as
  a separately tracked integrated-branch blocker; do not modify that path.

## Non-goals

- No public API, CLI option, Card/schema, storage format, audit table,
  publisher protocol, queue, timeout, workload, SLO, report column, capacity
  claim, or configuration surface.
- No blocking audit commit, cross-process barrier, distributed snapshot,
  transaction spanning metrics and SQL, or new durable owner.
- No redesign of run settlement, Drift execution, Oracle audit production,
  metrics transport, or publication.
- No external test target, new test harness, duplicated fixtures, or unrelated
  test reorganization.
- No change to replica cleanup or other behavior already accepted under
  FIND-16.

## Acceptance criteria

### AC-R5-1 — an otherwise-empty drain is bracketed against later audit production

Before a poll returns drained, it has both the existing pre-query pending
observation and a current post-query pending observation from every serving
replica. A decision already pending before SQL, handed off during SQL, or first
created after the pre-query observation keeps the audit backlog nonzero until
it is committed or counted lost. A queued run that has not yet produced its
decision remains visible as a run backlog in the SQL snapshot. The returned
final scrape is the observation that permitted the zero decision.

Closes `FIND-TASK-008-CLOSEOUT-17`.

### AC-R5-2 — the real queued-verifier path proves the snapshot interval

A focused repository-managed Postgres proof causes a queued Drift run to stage
its audited Oracle decision after the first replica observation, holds the
audit commit while the run settles, and demonstrates that the drain cannot
accept the intervening zero durable snapshot. After release, the same audit
cell stays nonzero while the row is above the publication watermark and
reaches zero only after publication settles. Existing pending-before-scrape,
post-stop-row, and exact 60-second proofs remain intact.

Closes the proof gap in `FIND-TASK-008-CLOSEOUT-17`.

### AC-R5-3 — environment-owned proof uses the required test boundary

The Postgres/live-server helpers and real audit-handoff proof live under an
in-source `#[cfg(test)] mod pg_tests` or an earned `pg_*` source file. Pure
metrics and backlog arithmetic remain in the ordinary test module. The live
proof retains its environment gate and exact repository-managed command, and
the default capacity selection does not execute it.

Closes `FIND-TASK-008-CLOSEOUT-18`.

### AC-R5-4 — adjacent behavior remains unchanged

The complete capacity target, exact drain edge, process-lifecycle proof,
canonical audit/loss/publication behavior, report schema, and relevant server
journeys retain their behavior. No public or durable contract is added.

## Focused proof and broader verification

Run the renamed queued-verifier handoff proof with its complete exact selector
through the repository Postgres wrapper and migrations. Its assertions must
cover: decision creation after the first observation; terminal run settlement
while commit is held; refusal to accept the intervening empty SQL snapshot;
pending-to-staging continuity; a staged row above `published_seq`; and zero
only after publication advances the watermark.

Retain and run the pure pending-plus-staged arithmetic test and the exact
60-second drain-edge test with exact `mise exec -- cargo nextest run --locked`
selectors. Run the complete `wyrd-testing` capacity target with its
environment-owned tests, the owning server/audit journey lane, the focused
`release_server` selection, `mise run fmt`, `mise run lints`, and
`git diff --check`. Use the repository-managed setup wrapper for every test
that needs Postgres or a live server. Do not substitute the deferred full
default benchmark for the focused queued-producer proof, and do not claim
empirical AC-040/AC-041 qualification.
