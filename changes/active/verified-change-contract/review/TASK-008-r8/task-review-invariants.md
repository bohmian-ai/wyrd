# TASK-008 r8 invariant review

## Subject and scope

- **Candidate:** `c4bc77a5c877c508191dc606b3cd3bb78047dc29`
- **Remediation range:** `ea0ed46fad8aa9ffa38c29c1b2d093d9d06177e8..c4bc77a5c877c508191dc606b3cd3bb78047dc29`
- **Approved authority:** `changes/active/verified-change-contract/spec.md`, revision 57, especially REQ-171
- **Original task:** `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- **Prior review:** `changes/active/verified-change-contract/review/TASK-008-r7/`
- **Remediation authority:** `TASK-008-CLOSEOUT-R6-close-final-audit-handoff.md`

The candidate remained at the stated commit before and after this review. The
range changes only the R6 remediation record and
`crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs`. Per caller direction,
this review tests closure of `FIND-TASK-008-CLOSEOUT-17` and regressions caused
by that range only. Earlier accepted TASK-008 code is used only to trace the
changed invariant. `FIND-TASK-008-CLOSEOUT-13` and the complete default
`bench:capacity` run remain deferred to integration and are not a failure here.

## Invariant trace

### Producer and authority transfer

`OracleQueryAudit::stage` increments the process-local pending count and
`audit_outbox_pending` gauge before enqueueing a decision
(`query_audit.rs:107-117`). `OracleAuditWriter::run` awaits
`commit_batch`, whose successful tenant path completes `conn.commit()`, before
decrementing the pending count and gauge (`query_audit.rs:162-178,187-211`). A
decision therefore transfers in this order:

```text
replica pending gauge > 0
  -> vala.audit_staging commit
  -> replica pending gauge decrement
  -> tenant publication watermark advancement
```

`Queue::backlog` counts every staged row above its tenant publication
watermark without applying the step stop time to audit rows
(`evidence.rs:264-304`). The single SQL statement observes staging rows and
watermarks under one PostgreSQL statement snapshot. `Backlog::with_replicas`
adds the replica pending gauges to that durable value
(`evidence.rs:174-189`). These existing owners give the drain continuous,
possibly conservatively double-counted ownership across the handoff.

### Corrected terminal observation

`Queue::poll` now evaluates the normal `S1 -> Q1` pair and preserves the prior
early return when it is nonempty. Only an otherwise-terminal poll proceeds to
`S2 -> Q2`, and it returns `S2 + Q2` (`evidence.rs:332-352`). In the exact r7
failure schedule:

1. `S1` sees no pending decision.
2. The decision is staged in process, but `Q1` fixes a snapshot before its
   staging commit and sees zero.
3. The writer commits the staging row, then decrements the pending gauge.
4. `S2` sees zero pending.
5. The new `Q2`, taken after `S2`, sees the committed row above the publication
   watermark and returns nonzero.

If publication commits before `Q2`, zero is correct because the row is no
longer owed. If the decision is still pending at `S2`, the returned replica
value is nonzero. PostgreSQL makes the row/watermark part of `Q2` internally
coherent, and the producer's commit-before-decrement order prevents a gap
between `S2` and `Q2` for the decision that crossed `Q1`.

`Deployment::run` waits for every public-client lane and its request tasks to
finish before capturing `stopped` and entering the drain
(`step.rs:330-360`; `load.rs:273-334`). Queued activations remain represented
until every expected run exists and is terminal. Thus the empty `Q1` condition
also closes the step-owned producer set before the second observation pair;
the correction does not assume arbitrary future traffic has stopped merely
because a metric scrape was empty. `Deployment::drain` consumes the changed
poll and accepts zero only through `Drain::judge`, preserving the exact
60-second boundary (`step.rs:395-423`).

### Focused proof

The Postgres proof drives the public Oracle client with audit publication
disabled. Its new phase holds `vala.audit_chain_head`, creates the decision
after `S1`, proves the durable state is still zero while the writer is blocked,
releases the lock inside the second scrape, waits until the post-commit pending
gauge is zero, and then requires the poll to return audit backlog one
(`evidence.rs:725-776`). It next proves the row remains owed above the
watermark and reaches zero only after the real publisher advances the watermark
(`evidence.rs:777-786`). This is the specific schedule the r7 proof omitted.
The earlier still-pending and pending-to-durable phases remain in the same
test (`evidence.rs:656-723`).

The proof is sensitive to the correction: without the second durable read,
the asserted `crossed.audit == 1` would receive the stale zero from `Q1` plus
the zero `S2`. It does not substitute an in-process fake producer or bypass the
canonical audit writer/publisher.

## Acceptance matrix

| Requirement, criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-171: a step-caused audit backlog must not be reported drained before its staging/publication lifecycle is complete | Producer commits before pending decrement (`query_audit.rs:162-178`); `Queue::poll` pairs terminal `S2` with fresh `Q2` (`evidence.rs:332-352`) | Held-commit proof source covers `S1=0, Q1=0, commit/decrement, S2=0, Q2=1`, then publication-to-zero (`evidence.rs:725-786`) | PASS |
| AC-R6-1: close the post-snapshot pending-to-durable handoff without an unbounded inner loop | Existing `Queue::backlog` is called once after `S2`; repetition and deadline remain in `Deployment::drain` | Source trace above; the returned value is the latest `Q2 + S2`, not stale `Q1 + S2` | PASS |
| AC-R6-2: use the public Oracle held-commit proof for both handoff sides | Existing still-pending phases remain; new crossed-commit phase uses `Bifrost::query_only`, the real writer, `AuditPublisher`, and the real staging/watermark tables | Focused Postgres command was attempted but could not start the repository Postgres fixture because this review environment cannot access its Docker socket. The candidate's R6 evidence records the exact proof passing and the source assertions directly exercise the missing schedule | PASS, with environment limit |
| AC-R6-3: preserve the exact deadline, adjacent backlog evidence, placement, and public/durable contracts | No production/server/API/schema/config change; early nonempty poll path, `Deployment::drain`, `Drain::judge`, report shape, and test placement are unchanged | Exact pure tests passed: `pending_decisions_add_to_staged_audit_rows` and `a_backlog_drains_only_within_the_limit` (2/2) | PASS |
| No regression introduced by `ea0ed46f..c4bc77a5` | The only executable change is the terminal poll's additional reuse of `Queue::backlog`; it is conditional on the first combined reading being empty and can only replace a false zero with fresher evidence (or conservatively defer termination) | `mise run fmt:check` passed; targeted `cargo clippy --locked -p wyrd-testing --bin capacity -- -D warnings` passed; `git diff --check` passed | PASS |
| Preserve non-blocking audit, canonical staging, single publisher, tenant watermarks, and counted-loss behavior | No producer, publisher, transaction, queue, gauge, or database query contract was changed | Diff and producer-to-sink trace | PASS |
| Keep FIND-13/default benchmark qualification deferred; do not claim AC-040/AC-041 qualification | The range contains no default benchmark-result artifact or qualification claim | Caller direction and R6 record explicitly retain the deferral | PASS |

## Proposed findings

None. The range contains no `MISSING`, `INCORRECT`, `DRIFT`, `VIOLATION`, or
`REGRESSION` within the directed closure scope.

The unavailable Docker-backed rerun is a verification limit, not a code
finding: the immutable source contains a decision-complete, real-Postgres proof
of the exact handoff, and the prior implementation record supplies its passing
execution result. This reviewer did not run or qualify the deferred default
capacity benchmark.

## Prior-finding closure and result

| Prior finding | Result |
|---|---|
| `FIND-TASK-008-CLOSEOUT-17` | **CLOSED.** The terminal poll now takes the required fresh durable observation after the final replica scrape, and the public held-commit proof fixes the commit/decrement transition between `Q1` and `S2`. |
| `FIND-TASK-008-CLOSEOUT-13` | **DEFERRED TO INTEGRATION** by caller direction; not reassessed. |

**Overall invariant-review result: PASS.**
