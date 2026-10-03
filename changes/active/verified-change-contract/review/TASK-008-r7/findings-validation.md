# TASK-008 round-seven findings validation

## Immutable subject and validation boundary

- Repository:
  `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `345295d8e`
- Candidate: `54373c36856d41ffe3554c977c35e9d033918cdd`
- Reviewed range:
  `345295d8e..54373c36856d41ffe3554c977c35e9d033918cdd`
- Approved authority:
  `changes/active/verified-change-contract/spec.md`, revision 57
- Original task:
  `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior decision and validation:
  `changes/active/verified-change-contract/review/TASK-008-r6/`
- Remediation task:
  `changes/active/verified-change-contract/review/TASK-008-r6/TASK-008-CLOSEOUT-R5-bracket-audit-drain-and-place-pg-proof.md`

The candidate remained at the named commit throughout validation. The checkout
has no `.codegraph/` directory, so navigation used Git, `rg`, and direct source
inspection. I read the complete four-file remediation diff, the applicable
repository and Bifrost authorities, the revision-57 capacity contract, the
original task's capacity closeout, the round-six verdict and validation, the
R5 remediation, and every round-seven discovery and follow-up report.

This validation follows the caller's closure boundary. It decides only whether
`FIND-TASK-008-CLOSEOUT-17` and `FIND-TASK-008-CLOSEOUT-18` are closed and
whether `345295d8e..54373c36856d41ffe3554c977c35e9d033918cdd` introduces a
regression. Earlier accepted code is considered only where its interaction with
this range can make the benchmark report a false PASS or FAIL.
`FIND-TASK-008-CLOSEOUT-13` remains deferred to integration and supplies no
empirical capacity qualification here.

No Cargo, `mise`, codegen, database-backed, or benchmark command was run in
this validation pass.

## Producer-to-consumer validation

### Audit ownership and the terminating poll

The unchanged production owner raises its pending count and
`audit_outbox_pending` before enqueueing a decision
(`crates/wyrd/wyrd-server/src/oracle/query_audit.rs:101-116`). The writer
awaits the canonical staging transaction, then decrements the count and gauge
only after commit or counted loss (`query_audit.rs:155-215`). A successful
decision therefore moves in this order:

1. the serving process owns a nonzero pending decision;
2. `vala.audit_staging` commits the decision;
3. the process releases the pending gauge; and
4. the staging row remains owed until `AuditPublisher` advances the tenant
   watermark.

`Queue::backlog` reads one PostgreSQL statement snapshot containing missing or
nonterminal runs, staged audit rows above each tenant watermark, and Forge
demand (`crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:278-305`).
`Backlog::with_replicas` adds process-local audit ownership and Scribe state
from a supplied metrics scrape (`evidence.rs:180-205`). Its own contract
correctly states that the no-miss pending-to-durable handoff requires the
scrape to precede the durable read (`evidence.rs:186-192`).

The new `Queue::poll` executes `S1 -> Q1`, combines them, and, only when that
combination is empty, executes `S2` and combines `S2` with the already captured
`Q1` (`evidence.rs:324-341`). This closes one part of the round-six gap: a
decision created after `S1` but still pending at `S2` keeps the audit cell
nonzero. It does not close the pending-to-staging handoff after `Q1` has fixed
its MVCC snapshot:

1. `S1` reads pending zero.
2. An already accepted queued Drift run reaches Oracle, whose authorization
   decision is staged before query execution
   (`crates/vala/vala-bifrost-redux/src/oracle/mod.rs:2211-2239,2723-2745`).
3. The query, result publication, and run settlement complete while the
   independent audit writer still owns the decision
   (`crates/wyrd/wyrd-server/src/verification/drift.rs:951-988`;
   `verification/runner.rs:241-297,372-437,628-702`).
4. `Q1` fixes a snapshot in which the run is terminal and the audit row is not
   committed, so its run and durable-audit values are zero.
5. After that snapshot, the writer commits the audit row and then decrements
   pending.
6. `S2` reads pending zero. `Queue::poll` reuses the stale `Q1` zero and returns
   empty even though a committed row remains above the publication watermark.

The producer is reachable in the revision-57 workload. The driver joins a
queued request after `start_run` accepts it, not after the runner finishes
(`capacity/load.rs:300-325,351-360,400-405`). `Q1` seeing all accepted runs
created and terminal establishes that no queued run remains able to become a
new producer after that snapshot; it does not make a commit after the snapshot
visible to that snapshot. `Deployment::drain` immediately hands the returned
empty value to `Drain::judge`, and `Drained` ends polling
(`capacity/step.rs:406-420`). The committed unpublished row can therefore make
revision-57 REQ-171's audit saturation cell falsely PASS.

### Focused proof fidelity

The integrator-approved public Oracle read is a valid proof vehicle. It uses the
same `OracleQueryAudit`, pending gauge, canonical staging append, and publisher
as the queued Drift reader, and the drain cannot distinguish which authorized
surface produced a pending decision.

The current proof captures `S1`, issues the public read, and waits until the
writer is blocked by the held chain-head transaction before `Q1`
(`capacity/evidence.rs:637-652`). It keeps that transaction held until after
`Queue::poll` returns and asserts `late.audit == 1`
(`evidence.rs:653-671`). It therefore proves exactly `S1 = 0, Q1 = 0, S2 = 1`.
It structurally excludes `S1 = 0, Q1 = 0, commit/decrement, S2 = 0`, which is
the remaining terminating-poll defect. The later aggregate handoff assertions
begin with two decisions and at least one nonzero owner; they do not exercise a
zero-to-one durable handoff at the decision that stops the poll.

### Test placement and fixed-port scheduling

The Postgres/live-server helpers and proof now reside in
`#[cfg(test)] mod pg_tests` (`capacity/evidence.rs:458-708`), while pure
backlog, metrics, and percentile tests remain in ordinary `mod tests`
(`evidence.rs:376-456`). The live proof retains its `#[ignore]` environment
gate. This is the exact existing repository mechanism required by
`architecture/agent-rules.md:15-17`; `FIND-TASK-008-CLOSEOUT-18` is closed
without another test binary, dependency, or duplicated fixture.

The added nextest group is also scoped and justified. The two selected capacity
tests both start the replica-zero stand-in on the release server's fixed port
8080 (`capacity/main.rs:790-792,830-860,920-950`;
`crates/wyrd/wyrd-testing/src/release_server.rs:31-34`), and no third capacity
test starts that stand-in. `.config/nextest.toml:62-70` serializes exactly those
two test names under the default profile using the repository's existing test
group mechanism. It changes only test scheduling and does not ignore, weaken,
retry, or extend either test.

## Proposed-finding decisions

| Proposal or conclusion | Decision | Validation |
|---|---|---|
| Behavior review's empty proposal set | **REVISED** | Its Oracle producer, `pg_tests`, and fixed-port traces stand, but it treats a decision as covered merely because it is either pending or durable without accounting for the commit after `Q1`'s snapshot and gauge decrement before `S2`. |
| `INV-R7-001` | **CONFIRMED** and deduplicated into prior `FIND-TASK-008-CLOSEOUT-17` | The source-ordered `S1 -> Q1 -> commit/decrement -> S2` interleaving is reachable and makes the terminating audit cell falsely empty. |
| Standards review's empty proposal set | **CONFIRMED as empty for repository-rule regressions; acceptance conclusion REVISED** | The changed code follows owner, async, documentation, test-placement, and nextest rules. Its conclusion that FIND-17 closes is contradicted by the durable handoff trace, not by a separate standards violation. |
| Maintainer review's empty proposal set | **CONFIRMED as empty for maintainability; closure conclusion REVISED** | The existing `Queue` owner, focused callback seam, module split, naming, and nextest group are maintainable. The remaining defect is correctness of the observation sequence, not a second maintenance finding. |
| `SYS-R7-01` | **CONFIRMED** and deduplicated into prior `FIND-TASK-008-CLOSEOUT-17` | It identifies the same terminating-poll handoff and the same false benchmark PASS as `INV-R7-001`. |
| Concurrency review's empty proposal set | **REVISED** | Its claim that a terminal run leaves the decision either visible to `Q1` or to `S2` omits the commit-before-decrement transition after `Q1` fixes its snapshot. Oracle staging is synchronous only as an enqueue; the commit is deliberately non-blocking. |
| `DUR-R7-001` | **CONFIRMED** and deduplicated into prior `FIND-TASK-008-CLOSEOUT-17` | PostgreSQL MVCC and the writer's commit-before-decrement order establish the same missed durable row. |
| `FOLLOWUP-R7-001` | **CONFIRMED** and deduplicated into prior `FIND-TASK-008-CLOSEOUT-17` | The follow-up correctly resolves the discovery conflict and confirms that the approved Oracle producer is sound but the held-commit timing proves only the still-pending half of the interval. |
| Discovery conclusions that FIND-18 closes | **CONFIRMED** | The complete environment-owned proof moved under `pg_tests`, pure tests remain under `tests`, and the execution gate remains intact. |
| Discovery conclusions that the fixed-port group is scoped | **CONFIRMED** | Exact selectors cover the only two capacity tests that bind the same fixed stand-in port; the repository-native one-thread group is the minimum scheduling correction. |

No proposal supports a new finding independent of the retained FIND-17.

## Ponytail correction analysis

Deleting the process gauge, second scrape, or durable query would recreate a
known false-zero interval. Existing owners already expose all required facts;
the standard library, native platform, and installed dependencies cannot make
two observations on opposite sides of an asynchronous handoff atomic. A new
queue, ledger, transaction spanning metrics and SQL, blocking request, endpoint,
configuration knob, or queued-Drift fixture would add scope without closing the
gap more directly.

The minimum correction remains on existing `Queue::poll`. Preserve `S1` and
`Q1`. If their combination is empty, take `S2` as the current replica evidence.
If `S2` is nonzero, return nonzero as today. If `S2` is zero, obtain a fresh
`Queue::backlog` durable observation after `S2` and combine that fresh durable
value with `S2` before accepting zero. A decision pending at `S2` already forces
another outer poll; a decision whose commit crossed `Q1` before the zero `S2`
must be visible to the later durable read unless it has genuinely published or
was counted lost. `Q1`'s zero run state bounds the producer side, so no new
barrier or unbounded observation loop is required. Keep `Deployment::drain`,
the 60-second judge, non-blocking audit semantics, canonical staging, and the
publisher unchanged.

The smallest credible proof continues to reuse the public Oracle held-commit
harness. Create the decision after `S1`; let `Q1` establish the empty snapshot;
then release the held commit and wait for the pending gauge to fall before the
terminating replica observation. Prove that the fresh durable observation
still returns a nonzero audit cell from the unpublished staging row, and that
the cell reaches zero only after publication advances the watermark. Retain the
existing still-pending, late-row, publication, exact-deadline, and broader
capacity-target checks. No Drift-specific harness is required.

## Final deduplicated finding ledger

### `FIND-TASK-008-CLOSEOUT-17` — REVISED — INCORRECT

- **Discovery source IDs:** `INV-R7-001`, `SYS-R7-01`, `DUR-R7-001`,
  `FOLLOWUP-R7-001`.
- **Violated obligation:** revision-57 REQ-171 requires every step-caused audit
  outbox backlog to drain within 60 seconds before the step passes. R5 AC-R5-1
  requires a terminating poll to cover later decision production through
  commit or counted loss, and the integrator-narrowed AC-R5-2 requires the real
  held-commit proof to demonstrate that false-empty interval.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:324-341` and
  `evidence.rs:637-671`, composed with
  `crates/wyrd/wyrd-server/src/oracle/query_audit.rs:155-178` and the terminal
  consumer at
  `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:406-420`.
- **Evidence:** `Queue::poll` reuses `Q1` after `S2`. A decision created after
  `S1` can commit after `Q1` fixes its MVCC snapshot, then leave the pending
  gauge before `S2`; both retained values are zero while
  `vala.audit_staging` owns an unpublished row. The proof holds the commit
  across `S2`, so it cannot fail under this handoff.
- **Observable consequence:** `bench:capacity` can end the drain, record audit
  backlog zero, and PASS REQ-171's saturation SLO while a step-caused audit row
  remains committed above the tenant publication watermark.
- **Decision-complete correction:** preserve the existing process-local owner,
  gauge, pre-query scrape, durable query, post-query scrape, non-blocking audit
  semantics, `Deployment::drain`, deadline, and publisher. Only on the
  otherwise-empty path, pair a zero post-query scrape with a fresh durable
  backlog read taken after that scrape before accepting zero. Reuse
  `Queue::backlog`; do not add an owner, queue, barrier, transaction, endpoint,
  configuration surface, or producer-specific production path.
- **Focused closure proof:** reuse the public Oracle held-commit harness. Stage
  after `S1`, allow `Q1` to observe no committed row, release and complete the
  commit before the final pending observation, assert pending is zero, and
  prove the poll remains nonzero from the fresh durable row until publication
  advances the watermark. Retain the existing still-pending and
  publication-to-zero phases and the exact 60-second drain proof.

No new finding is assigned.

## Prior-finding closure

| Prior finding | Validation result |
|---|---|
| `FIND-TASK-008-CLOSEOUT-17` | **OPEN.** The range catches a decision created after the first scrape while it remains pending, but can still miss the same decision when its commit crosses the first SQL snapshot and its gauge falls before the second scrape. |
| `FIND-TASK-008-CLOSEOUT-18` | **CLOSED.** The live-server/Postgres proof and helpers are under `pg_tests`, pure tests remain under `tests`, and the environment gate is retained. |
| `FIND-TASK-008-CLOSEOUT-13` | **DEFERRED** to integration by caller direction; it is non-blocking here and provides no AC-040/AC-041 qualification. |

## Verification limits and validation result

- The remediation record reports the focused Postgres proof, pure arithmetic
  and drain-edge tests, default capacity selection, three complete ignored
  capacity-target runs, release-server tests, server Bifrost journey, format,
  lints, and diff check passing. Those results were not rerun in this pass.
- Those green runs do not exercise the confirmed
  `Q1 -> commit/decrement -> S2` schedule because the focused proof keeps the
  commit blocked until after `S2`.
- The complete unmodified default `mise run bench:capacity` remains deferred
  as `FIND-TASK-008-CLOSEOUT-13`.

The final ledger retains one bounded finding,
`FIND-TASK-008-CLOSEOUT-17`. `FIND-TASK-008-CLOSEOUT-18` is closed, and the
range's nextest serialization introduces no validated regression. The retained
correction uses the existing `Queue`, replica gauge, canonical durable query,
and public Oracle proof harness and requires no specification or architecture
decision.
