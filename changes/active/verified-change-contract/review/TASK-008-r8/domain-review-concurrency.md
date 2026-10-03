# Concurrency domain review

## Immutable subject and boundary

- Candidate: `c4bc77a5c877c508191dc606b3cd3bb78047dc29`
- Remediation range: `ea0ed46fa..c4bc77a5c877c508191dc606b3cd3bb78047dc29`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior finding under closure review: `FIND-TASK-008-CLOSEOUT-17`
- Remediation authority: `changes/active/verified-change-contract/review/TASK-008-r7/TASK-008-CLOSEOUT-R6-close-final-audit-handoff.md`

This review is limited to the concurrency boundary changed by the remediation:
`Queue::poll`'s observations of replica-owned audit decisions and durable audit
staging, the Oracle writer's commit-to-gauge handoff, the outer
`Deployment::drain` deadline, and the held-chain-head proof. Earlier accepted
code is not reopened. `FIND-TASK-008-CLOSEOUT-13` and the full default
`bench:capacity` qualification remain deferred to integration.

The candidate remained at the immutable commit above while this report was
prepared.

## Authority coverage

| Authority | Applicable concurrency obligation | Result |
|---|---|---|
| Revision-57 `REQ-171` | Every step-caused audit decision must remain represented as backlog until commit loss or publication; a step may pass only after the backlog drains within 60 seconds. | PASS |
| R6 `AC-R6-1` | When `S1 + Q1` is empty, combine a final replica observation with a durable observation taken after it before accepting zero. | PASS |
| R6 `AC-R6-2` | Exercise the `Q1 -> commit/decrement -> S2` interleaving with the public Oracle held-commit harness and prove the row remains nonzero until publication. | PASS |
| R6 constraints | Preserve the outer drain loop, exact deadline, non-blocking audit semantics, canonical staging, publisher, and conservative over-counting. | PASS |
| `AGENTS.md` and `architecture/agent-rules.md` audit doctrine | A decision remains process-pending until committed or counted lost; committed rows remain in canonical staging until the publisher advances the watermark. | PASS |

## Source coverage

- `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:307-353`:
  full changed `Queue::poll` body and its concurrency argument.
- `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:163-205`:
  `Backlog` and `Backlog::with_replicas`, including pending-gauge addition.
- `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:261-305`:
  `Queue::backlog` and the staging-above-watermark durable observation.
- `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:473-790`:
  the complete repository-managed held-chain-head proof, including the added
  crossed-snapshot phase.
- `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:239-262` and
  `step.rs:383-423`: `Drain::judge` and the only production caller of
  `Queue::poll`, including exact-deadline and cancellation behavior.
- `crates/wyrd/wyrd-server/src/oracle/query_audit.rs:101-117` and
  `query_audit.rs:155-215`: stage ordering, writer lifecycle, commit/loss, and
  the commit-before-pending-decrement invariant.
- Complete range diff: only the R6 remediation record and
  `capacity/evidence.rs` changed. No producer, publisher, durable schema,
  deadline, or deployment owner changed in this range.

## Reachable schedule proof

Let `S1` and `S2` be the first and second all-replica scrapes, and `Q1` and
`Q2` the following PostgreSQL statement-snapshot reads. The candidate executes
`S1 -> Q1`; it returns immediately if their combined backlog is nonzero.
Otherwise it executes `S2 -> Q2` and returns only their latest combined value.

| Reachable state | Observation path | Result |
|---|---|---|
| Work is visible in `S1` or `Q1` | The first combined backlog is nonzero and returns immediately. | Safe conservative nonzero; the outer loop repeats. |
| A decision is created after `S1` and remains uncommitted through `S2` | `Q1` is zero, but `S2` reads the pending gauge as nonzero. | Nonzero. |
| A decision commits before `Q1` | `Q1` sees the staging row unless publication has already advanced past it. | Nonzero if still owed; correctly zero if already published. |
| A decision's commit crosses `Q1`, then its pending gauge decrements before `S2` | `S2` may be zero, but commit completion precedes the decrement and `Q2` starts after `S2`, so `Q2` sees the committed row unless it has already been published. | Nonzero if still owed. This is the previously false-zero interval and it is closed. |
| A decision is still pending at `S2` and commits before `Q2` | `S2` is already nonzero and `Q2` may additionally see the row. | Conservative over-count, never false zero. |
| Commit fails or enqueue refuses the decision | The Oracle owner counts the event lost before decrementing pending. No durable row is owed by the approved contract. | Correctly allowed to clear. |
| Publication races between `S2` and `Q2` | `Q2` excludes a row only after its tenant watermark covers it. | Correctly zero for that decision because its required publication completed. |
| Several replicas or decisions occupy different handoff phases | `S2` sums every replica's pending gauge; `Q2` counts every tenant's staging rows above its watermark. | Any still-pending or committed-unpublished decision keeps the cell nonzero. |
| All four observations are empty | `Q1` has established no outstanding/absent expected run producer in the benchmark window; after that boundary each already-created audit decision is either visible at `S2`, visible at `Q2`, counted lost, or published. | Zero is sound for the reviewed boundary. |

The key happens-before edge is in `OracleAuditWriter::run`: the awaited
`commit_batch` completes before both the atomic pending decrement and metrics
gauge decrement. Therefore a zero `S2` cannot precede an uncommitted successful
batch, and the later read-committed `Q2` cannot retain `Q1`'s old statement
snapshot.

The second read does not introduce an internal retry loop. Nonzero results
return to `Deployment::drain`, which retains ownership of repetition and the
absolute 60-second judgment. If either scrape or query is slow past the limit,
the elapsed time is checked after the poll: a nonempty result expires, and even
an empty result after the limit expires rather than passing. An error propagates
without a result. Cancelling/dropping the poll or drain produces no `Record` and
therefore no false PASS; both operations are read-only and leave producer-owned
work to continue draining.

## Focused proof assessment

The added phase at `evidence.rs:725-787` exercises the required interleaving:

1. hold the tenant chain-head lock;
2. capture zero at `S1`, then issue the public Oracle read;
3. wait until its writer is blocked, which places the decision after `S1` and
   before the durable observations;
4. on the `S2` callback, independently confirm durable audit zero while the
   commit remains held;
5. release the lock, wait until pending reaches zero, and return an `S2` scrape
   that is explicitly zero;
6. require the poll result to be audit backlog one, which can come only from
   `Q2` in that schedule;
7. prove a later read remains one above the watermark and becomes zero only
   after the real publisher advances it.

This proof would fail if the implementation reused `Q1` after `S2`. It also
retains the earlier phase where the decision stays pending through `S2`.

The focused repository-managed PostgreSQL command could not be rerun in this
review environment because access to the configured Docker socket was denied.
The candidate's remediation record reports the focused proof passing and a
deliberate red run without `Q2` failing at the expected assertion. The pure
focused command passed both selected tests after the shared Cargo build lock
became available: `pending_decisions_add_to_staged_audit_rows` and
`a_backlog_drains_only_within_the_limit` (2 passed, 18 skipped). The unavailable
PostgreSQL rerun is a verification limit, not contrary source evidence. The
integration-owned full default benchmark remains intentionally outside this
closure review.

## Findings

No material concurrency finding is proposed. The range introduces no
concurrency regression within the user-directed boundary.

`FIND-TASK-008-CLOSEOUT-17` is **CLOSED**: the candidate adds the post-`S2`
durable observation required to cover the successful pending-to-staging
handoff, and the held-chain-head proof drives that exact interleaving through
the public Oracle producer.

## Overall result

**PASS**
