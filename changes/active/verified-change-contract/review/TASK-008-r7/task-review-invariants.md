# TASK-008 round-seven invariant review

## Immutable subject and review boundary

- Approved specification:
  `changes/active/verified-change-contract/spec.md`, revision 57
  (`approved`).
- Original task:
  `changes/active/verified-change-contract/tasks/task-008-closeout.md`.
- Prior review:
  `changes/active/verified-change-contract/review/TASK-008-r6/`.
- Prior remediation:
  `TASK-008-CLOSEOUT-R5-bracket-audit-drain-and-place-pg-proof.md`.
- Base: `345295d8e`.
- Candidate: `54373c36856d41ffe3554c977c35e9d033918cdd`.
- Reviewed range:
  `345295d8e..54373c36856d41ffe3554c977c35e9d033918cdd`.

The candidate matched the named commit during this review. The checkout has no
`.codegraph/` directory, so navigation used Git, `rg`, and direct source
inspection.

Per caller direction, this review decides only whether
`FIND-TASK-008-CLOSEOUT-17` and `FIND-TASK-008-CLOSEOUT-18` are closed and
whether this range introduces a regression. Earlier accepted code is not
reopened except where it can make the benchmark's PASS/FAIL result false.
`FIND-TASK-008-CLOSEOUT-13`, the complete default `bench:capacity` run, remains
deferred to integration and provides no empirical capacity qualification here.

I read the complete remediation diff, both changed runtime/test modules and
their callers, the nextest configuration, the revision-57 capacity and audit
authorities, the original task's revision-57 benchmark section, and the R6
verdict, validation ledger, and remediation task. This was a source-only
review; the orchestrator owns command verification.

## Navigation and ownership map

| Surface | Owner and path | Consumers traced |
|---|---|---|
| Process-local audit ownership | `OracleQueryAudit::stage` and `OracleAuditWriter::run` in `crates/wyrd/wyrd-server/src/oracle/query_audit.rs:101-178` | Public Oracle reads and queued Drift's Oracle read; exported `audit_outbox_pending` gauge |
| Durable audit ownership | `Queue::backlog` in `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:261-305` | `Queue::poll`; canonical `vala.audit_staging` rows above each tenant's publication watermark |
| Poll composition | `Backlog::with_replicas` and `Queue::poll` in `capacity/evidence.rs:186-204,307-341` | Sole production consumer `Deployment::drain` |
| Drain verdict | `Deployment::drain` and `Drain::judge` in `capacity/step.rs:239-261,383-423` | Step `Record`, report backlog cell, and ultimately benchmark PASS/FAIL |
| Closure proof | `capacity/evidence.rs:458-708` | In-source ignored `pg_tests` proof using public Oracle reads, a held audit-chain commit, and manual publication |
| Fixed-port isolation | `.config/nextest.toml:62-70` | The two ignored capacity tests in `capacity/main.rs:819-960` that start the same replica-0 stand-in on port 8080 |

## Producer-to-sink invariant trace

### Audit decision lifecycle

The production audit owner increments its atomic pending count and
`audit_outbox_pending` gauge before enqueue
(`query_audit.rs:101-116`). The writer commits a received batch and only then
decrements pending and the gauge (`query_audit.rs:155-178`). A successful
decision therefore moves through these states:

1. process-local pending is nonzero and no staging row need exist;
2. the staging transaction commits;
3. process-local pending becomes zero;
4. the staging row remains above `published_seq` until `AuditPublisher`
   advances the watermark.

`Queue::backlog` correctly observes state 4 by counting all staging rows above
their tenant watermark (`evidence.rs:284-304`). `Backlog::with_replicas`
correctly observes state 1 from the scrape (`evidence.rs:186-204`). Neither
observation alone is authoritative during the handoff.

### The new bracket still has a false-empty handoff interval

`Queue::poll` performs one replica scrape, takes one PostgreSQL statement
snapshot, and, only when their combination is empty, performs a second replica
scrape (`evidence.rs:331-340`). This closes the specific interval in which a
later decision remains pending through the second scrape. It does not close a
later decision's pending-to-durable handoff after the SQL snapshot:

1. The first scrape reads pending zero because the queued run has not reached
   Oracle yet.
2. The run stages its Oracle decision and can settle terminally while the
   independent audit writer owns the decision. This is the accepted R6
   producer path.
3. The PostgreSQL statement snapshot reads the run terminal and reads no
   committed staging row, so the durable backlog is empty.
4. After that statement snapshot, the writer commits the staging row and then
   decrements `audit_outbox_pending`.
5. The second scrape reads pending zero. `Queue::poll` combines that scrape
   with the stale empty durable snapshot and returns an empty backlog.
6. `Deployment::drain` passes the empty result to `Drain::judge`, which can
   return `Drained`; the report can therefore record PASS while a committed,
   unpublished audit row exists.

This ordering follows the production writer's explicit commit-before-decrement
sequence and requires no crash or speculative producer. The first SQL snapshot
can establish that no queued run producer remains, but it cannot observe a
commit occurring after that snapshot. Once the post-query scrape is empty, a
second durable observation is needed to cover that already-existing handoff.

The added proof does not falsify this interval. Its callback issues the public
Oracle read after the first scrape and then deliberately waits until the writer
is blocked on the held chain-head lock (`evidence.rs:641-650`). The second
scrape must consequently observe pending one, and the assertion at lines
653-656 proves only that retained-pending case. It never releases the commit
between the durable snapshot and the second scrape, which is the remaining
false-empty ordering. The caller-approved use of a public Oracle read is
appropriate—the drain cannot distinguish which surface produced the decision;
the defect is that the proof holds the decision in only one of the two
ownership states across the interval.

### Test placement and fixed-port serialization

The environment-owned helpers and live Postgres/server proof now live in the
in-source `#[cfg(test)] mod pg_tests` at `evidence.rs:458-708`; the pure
arithmetic and percentile proofs remain in ordinary `mod tests` at lines
376-456. The live proof retains its environment `#[ignore]` gate. This closes
the exact source-classification violation in FIND-18 without adding an external
test binary or duplicating fixtures.

The nextest group is also justified and bounded. Both selected tests execute
the same stand-in server, whose literal listener is `127.0.0.1:8080`
(`capacity/main.rs:773-794`), and both start replica ordinal zero through the
release-server owner (`capacity/main.rs:831-860,921-960`). The override selects
exactly those two fully qualified tests in the `capacity` binary and serializes
them only in the default profile (`.config/nextest.toml:62-70`). No other test
in that binary starts the fixed-port release server. The range records the
observed `Address already in use` diagnosis and does not weaken, ignore, or
delete either proof. I found no regression from this configuration change.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-17 / AC-R5-1: an otherwise-empty drain must cover a decision pending before SQL, handed off during SQL, or first created after the first observation until commit/loss | `Queue::poll` adds a second scrape only after the first combined snapshot is empty (`evidence.rs:307-341`); production consumer is `Deployment::drain` (`step.rs:406-420`) | Source interleaving shows a later-produced decision can commit after SQL's snapshot and decrement before scrape two, yielding false zero | **FAIL** |
| FIND-17 / narrowed AC-R5-2: reuse the held-commit harness and issue a public Oracle read between scrape one and the durable query to prove the false-empty interval | The proof issues `oracle.sql` inside the first scrape callback and blocks its writer (`evidence.rs:637-656`) | The assertion proves pending survives to scrape two, but not pending-to-staging handoff after SQL's snapshot and before scrape two | **FAIL** |
| FIND-17: durable unpublished work remains nonzero through publication | `Queue::backlog` counts staging rows above `published_seq`; proof releases the fence and drives `AuditPublisher` (`evidence.rs:284-304,671-703`) | Source assertions cover post-stop commit, nonzero before publication, and zero afterward | **PASS** |
| FIND-18 / AC-R5-3: environment-owned proof uses `pg_tests`; pure tests remain in the ordinary module | `evidence.rs:376-456` contains pure tests; `evidence.rs:458-708` contains Postgres/server helpers and proof | Static module and `#[ignore]` inspection | **PASS — CLOSED** |
| The range's fixed-port nextest group addresses a diagnosed real conflict without hiding test behavior | Exact default-profile override selects the two capacity tests that bind replica-0 port 8080 (`.config/nextest.toml:62-70`; `capacity/main.rs:773-794,819-960`) | Source selection and caller inspection; recorded remediation evidence reports the pair reproduced the bind conflict and the serialized complete target passed | **PASS** |
| Preserve non-blocking audit, canonical staging/publication, workload, report, deadline, and prior accepted behavior | Range changes only benchmark observation composition, its proof organization/timing, and focused nextest scheduling; production audit owner is unchanged | Complete range and caller review | **PASS within source-review limits** |
| FIND-13 full default benchmark qualification remains deferred | No performance claim is added by the range | Explicit caller direction and remediation evidence | **PASS — DEFERRED, non-blocking here** |

## Proposed findings

### `INV-R7-001` — INCORRECT — retain `FIND-TASK-008-CLOSEOUT-17`

- **Violated obligation:** Revision-57 REQ-171 requires every step-caused audit
  backlog to be drained within 60 seconds before a step passes. AC-R5-1 requires
  coverage from later decision creation through commit or counted loss, and the
  narrowed AC-R5-2 proof must demonstrate the false-empty interval rather than
  merely the producer surface.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:331-340`, with the
  incomplete proof at `evidence.rs:641-656` and the PASS consumer at
  `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:406-420`.
- **Evidence:** the audit writer commits before decrementing pending
  (`crates/wyrd/wyrd-server/src/oracle/query_audit.rs:176-178`). A decision
  produced after scrape one can be absent from SQL's statement snapshot, commit
  after that snapshot, and decrement before scrape two. The returned backlog
  then combines two zero observations taken on opposite sides of a real staged
  row. The proof keeps the commit blocked until after scrape two, so it cannot
  fail under this reachable handoff.
- **Observable consequence:** a judged capacity step can record the audit
  backlog drained and PASS while `vala.audit_staging` contains an unpublished
  step-caused decision. The benchmark's saturation verdict is false.
- **Required testable correction:** preserve the existing pre-query scrape,
  durable query, and otherwise-empty post-query scrape. When that post-query
  scrape is also empty, obtain a current durable audit observation before
  accepting zero, so a commit between the first SQL snapshot and the second
  scrape remains visible. The first durable snapshot's zero run backlog is the
  boundary that establishes no later queued producer remains; do not add a
  blocking audit barrier or wait on requests. Extend the existing public-Oracle
  held-commit proof so the decision is created after scrape one, the first SQL
  snapshot observes neither pending nor staging as sufficient evidence, the
  held transaction is released and commits before the final pending scrape,
  and the drain still observes the now-durable unpublished row. Retain the
  pending-through-scrape and publication-to-zero phases.

No independent finding is proposed for FIND-18 or the nextest configuration.

## Prior-finding closure

| Finding | Result |
|---|---|
| `FIND-TASK-008-CLOSEOUT-17` | **OPEN.** The second scrape catches a later decision only while it remains process-local. A commit after the SQL snapshot and before that scrape crosses both observations and can still produce a false PASS. |
| `FIND-TASK-008-CLOSEOUT-18` | **CLOSED.** The live Postgres/server proof and helpers are under `pg_tests`; pure tests remain under `tests`. |
| `FIND-TASK-008-CLOSEOUT-13` | **DEFERRED** to integration by caller direction; not counted against this review. |

## Verification limits

- This reviewer ran no Cargo, mise, database, server, codegen, or benchmark
  command; the orchestrator owns sequential verification.
- The remediation evidence records the focused live proof passing, the pure
  arithmetic and drain-edge proofs passing, the default capacity selection at
  15 passed/5 skipped, three complete ignored capacity-target runs at 20/20,
  release-server tests at 2/2, the server Bifrost journey at 29/29, and format,
  lints, and diff checks clean. Those green results do not exercise the
  commit-after-SQL/before-scrape ordering above.
- The full default `mise run bench:capacity` remains deferred as FIND-13, so no
  AC-040/AC-041 performance qualification is inferred.

## Overall result

**FAIL**

`FIND-TASK-008-CLOSEOUT-18` is closed, and the nextest serialization introduces
no source-visible regression. `FIND-TASK-008-CLOSEOUT-17` remains open because
the poll and its proof do not cover the pending-to-durable handoff between the
durable statement snapshot and the post-query scrape; that interval can still
make the benchmark report a false PASS.
