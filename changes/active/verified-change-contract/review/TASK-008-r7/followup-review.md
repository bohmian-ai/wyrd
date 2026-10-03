# Focused follow-up review: audit handoff at the terminating drain poll

## Immutable subject and uncertainty

- Base: `345295d8e`.
- Candidate: `54373c36856d41ffe3554c977c35e9d033918cdd`.
- Range: `345295d8e..54373c36856d41ffe3554c977c35e9d033918cdd`.
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 57, especially `REQ-171`.
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`.
- Prior finding and remediation: `FIND-TASK-008-CLOSEOUT-17` in `review/TASK-008-r6/` and `TASK-008-CLOSEOUT-R5-bracket-audit-drain-and-place-pg-proof.md`.

This follow-up resolves one material discovery conflict. The PASS reports say
the new `S1 -> Q1 -> conditional S2` sequence and public-Oracle held-commit
proof close the later-producer false-empty interval. The FAIL reports say a
decision can be created after `S1`, remain pending when `Q1` fixes its
PostgreSQL snapshot, commit and drop its pending gauge before `S2`, and
therefore be absent from both observations that `Queue::poll` combines.

The user-directed closure boundary applies: this report does not reopen prior
accepted code except where its interaction with this range can make the
benchmark verdict false. `FIND-TASK-008-CLOSEOUT-13` remains deferred. The
integrator-approved public Oracle read is accepted as the proof producer; the
question is only whether the proof covers the complete ownership handoff.

The checkout has no `.codegraph/` directory, so navigation used `rg`, Git, and
direct source inspection. No Cargo, `mise`, codegen, database-backed, or
benchmark command was run.

## Source paths inspected

- All round-seven discovery reports:
  `task-review-behavior.md`, `task-review-invariants.md`,
  `standards-review.md`, `maintainer-review.md`, `system-review.md`,
  `domain-review-concurrency.md`, and `domain-review-durability.md`.
- Round-six authority and diagnosis:
  `verdict.md`, `findings-validation.md`, and
  `TASK-008-CLOSEOUT-R5-bracket-audit-drain-and-place-pg-proof.md`.
- Drain evidence and proof:
  `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:180-205,261-341,458-708`.
- Terminating caller and revision-57 workload ordering:
  `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:239-261,320-423` and
  `crates/wyrd/wyrd-testing/src/bin/capacity/load.rs:330-420`.
- Audit producer and handoff:
  `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:2210-2239,2720-2745`,
  `crates/wyrd/wyrd-server/src/oracle/query_audit.rs:101-215`,
  `crates/wyrd/wyrd-server/src/verification/drift.rs:925-990`, and
  `crates/wyrd/wyrd-server/src/verification/runner.rs:232-310,360-455,628-675`.
- Durable publication authority:
  `crates/wyrd/wyrd-server/src/audit/publication.rs:259-383` and
  `architecture/bifrost-design.md:587-640`.
- Scope and repository authority:
  `AGENTS.md`, `architecture/agent-rules.md`,
  `architecture/references/languages/spec-driven-development.md`,
  `architecture/references/languages/maintainer-style.md`, and revision-57
  `REQ-171` in `changes/active/verified-change-contract/spec.md`.

## Evidence resolving the conflict

### The audit handoff has two distinct intervals

`OracleQueryAudit::stage` increments the process-local count and
`audit_outbox_pending` before enqueue
(`query_audit.rs:101-116`). `OracleAuditWriter::run` awaits
`commit_batch` and only then decrements the count and gauge
(`query_audit.rs:155-178`). The successful ownership sequence is therefore:

1. pending gauge is nonzero while no committed staging row is required;
2. the staging transaction commits;
3. the writer decrements the pending gauge; and
4. the committed row remains durable backlog until publication advances the
   tenant watermark.

The first scrape safely covers a decision already pending at `S1`: if it
commits during `Q1`, the retained `S1` value conservatively keeps that poll
nonzero. The new second scrape also safely covers a decision created after
`S1` that remains pending through `S2`. These are the schedules exercised by
the PASS reports and by the added held-commit assertion.

They are not the disputed schedule. `Queue::backlog` executes one SQL statement
and returns one statement snapshot of run state and committed audit staging
(`evidence.rs:278-305`). `Queue::poll` then reuses that same result after `S2`
(`evidence.rs:331-340`); it performs no durable read after `S2`. The source's
own `Backlog::with_replicas` contract says its no-miss handoff guarantee is
conditional on the replica scrapes occurring before the durable read
(`evidence.rs:186-203`). Applying it to `S2`, which occurs after `Q1`, does not
satisfy that condition.

A commit after `Q1` has fixed its MVCC snapshot is invisible to `Q1`, even if
the commit completes before the statement returns. Once that commit returns,
the writer is free to decrement the pending gauge before `S2`. `S2` can
therefore read zero while the row is committed above `published_seq`; the
implementation combines that zero with the stale durable zero and returns an
empty `Backlog`. `Deployment::drain` immediately gives the result to
`Drain::judge`, and `Drained` ends the loop, so no later poll repairs the
observation (`step.rs:406-420`).

### The disputed ordering is reachable in the revision-57 workload

The capacity driver joins request-lane futures before entering drain
(`step.rs:330-360`), but a queued request's successful client response is an
enqueue acknowledgement, not completion of the verifier run. The fixed
`activations` count and the run subqueries in `Queue::backlog` cover accepted
work until its run exists and while it is nonterminal
(`evidence.rs:284-303`). They do not keep a run nonterminal until its
independent audit commit completes.

For queued Drift, Oracle stages the read decision before binding and executing
the query (`vala-bifrost-redux/src/oracle/mod.rs:2220-2239`). Drift awaits the
query stream, then the runner publishes the result and settles the run in a
separate transaction (`verification/drift.rs:951-988`;
`verification/runner.rs:241-297,628-665`). The audit writer is a separate
tracked task and the query does not await its commit. Thus this ordering needs
no failure, restart, new external arrival, or speculative producer:

1. `S1` observes pending zero.
2. An already accepted queued Drift run reaches Oracle; Oracle stages its
   decision after `S1`.
3. The query, result publication, and run settlement complete while the audit
   writer still owns the decision.
4. `Q1` fixes a snapshot in which the run is terminal and the audit row is not
   committed, so both run and audit cells are zero.
5. The audit transaction commits after that snapshot; the writer then
   decrements pending.
6. `S2` observes pending zero, and `Queue::poll` returns the reused `Q1` zero.

The first durable zero is still useful: it proves there is no missing or
nonterminal queued run able to become a new producer after that snapshot.
That fact makes the correction bounded. It does not make a row committed
after the snapshot visible to the already-completed observation.

### The public Oracle producer is valid, but the proof holds the wrong side of the handoff

The integrator-directed substitution is sound in kind. The public Oracle read
uses the same `OracleQueryAudit` owner, gauge, canonical staging path, and
publisher as Drift's Oracle read; the drain cannot identify the originating
surface.

The current proof captures `S1`, issues the public read, waits until the writer
is blocked on the held chain-head transaction, and keeps that lock held until
after `Queue::poll` returns and `late.audit == 1` is asserted
(`evidence.rs:637-671`). Consequently the decision must still be pending at
`S2`. That proves only `S1=0, Q1=0, S2=1`. It structurally excludes the
disputed `S1=0, Q1=0, commit/decrement, S2=0` ordering.

The later loop after `fence.commit()` does not close the proof gap. It starts
with two held decisions, repeatedly invokes a fresh complete poll, and asserts
aggregate continuity while rows become staged (`evidence.rs:671-697`). At
least one nonzero pending or durable value already exists throughout that
phase, so it does not test the otherwise-empty terminating poll that reuses a
pre-commit `Q1` snapshot.

The PASS reports establish that the new proof detects a decision retained in
the pending state, but they do not trace the commit-after-snapshot timing.
Their conclusion that every handoff is covered conflates “created after `S1`
and still pending at `S2`” with “created after `S1` and transferred to durable
ownership after `Q1` but before `S2`.” The invariant, system, and durability
reports correctly distinguish those schedules.

## Proposed finding

### `FOLLOWUP-R7-001` — INCORRECT — retain `FIND-TASK-008-CLOSEOUT-17`

- **Violated obligation:** revision-57 `REQ-171` requires every step-caused
  audit-outbox backlog to drain within 60 seconds before a step passes.
  `FIND-TASK-008-CLOSEOUT-17` requires the terminating poll and its proof to
  exclude a false zero across the process-pending to durable-staging handoff.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:331-340`, with the
  incomplete proof at `evidence.rs:637-671`, the handoff at
  `crates/wyrd/wyrd-server/src/oracle/query_audit.rs:176-178`, and the terminal
  consumer at `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:417-420`.
- **Evidence:** a decision created after `S1` can commit after `Q1` fixes its
  PostgreSQL snapshot and decrement pending before `S2`. `Q1` cannot see the
  commit, `S2` sees no pending owner, and `Queue::poll` reuses `Q1` rather than
  observing durable state again. The proof holds the commit until after `S2`
  and therefore cannot fail under this schedule.
- **Observable consequence:** `bench:capacity` can stop polling, record audit
  backlog zero, and PASS the REQ-171 saturation cell while a step-caused audit
  row is committed but still unpublished above the tenant watermark.
- **Bounded correction:** retain the existing audit owner, gauge, canonical
  staging query, non-blocking request semantics, `Deployment::drain` owner,
  deadline, and public Oracle harness. Before accepting zero after `S2`, obtain
  a fresh durable observation after `S2`; this observes a commit that crossed
  `Q1`, while a decision still pending at `S2` already forces another loop.
  Extend the held-commit proof so its second callback releases the fence after
  `Q1` has returned, waits for the pending gauge to fall, and demonstrates that
  the terminating poll remains nonzero from the newly committed staging row
  until publication. No queued-Drift fixture, production audit change,
  blocking barrier, transaction spanning metrics and SQL, public contract, or
  new lifecycle owner is required.

This is the existing scoped finding remaining open, not a new defect in
earlier accepted code. `FIND-TASK-008-CLOSEOUT-18` and the fixed-port nextest
group are not implicated by this uncertainty.

## Resolution

**RESOLVED.** The material conflict resolves in favor of the invariant,
system-resilience, and durability analyses. The `S1 -> Q1 -> S2` sequence
closes a later producer that remains process-pending, but it does not close a
successful pending-to-staging handoff after `Q1`'s snapshot and before `S2`.
The public Oracle producer is appropriate; the current held-commit timing does
not exercise the full false-empty interval. `FIND-TASK-008-CLOSEOUT-17`
therefore remains open with a bounded consumer-side correction and focused
proof. No separate proposed finding arises for `FIND-TASK-008-CLOSEOUT-18`,
the fixed-port serialization, or deferred `FIND-TASK-008-CLOSEOUT-13`.
