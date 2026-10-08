# TASK-008 round-eight behavior review

## Subject and scope

- Candidate: `c4bc77a5c877c508191dc606b3cd3bb78047dc29`.
- Remediation range:
  `ea0ed46fad8aa9ffa38c29c1b2d093d9d06177e8..c4bc77a5c877c508191dc606b3cd3bb78047dc29`.
- Approved authority:
  `changes/active/verified-change-contract/spec.md`, revision 57.
- Original task:
  `changes/active/verified-change-contract/tasks/task-008-closeout.md`.
- Prior review:
  `changes/active/verified-change-contract/review/TASK-008-r7/`.
- Remediation task:
  `TASK-008-CLOSEOUT-R6-close-final-audit-handoff.md`.

The candidate remained at the named commit throughout this review. The checkout
has no `.codegraph/` directory, so navigation used Git, `rg`, and direct source
inspection. Per user direction, this review decides only whether
`FIND-TASK-008-CLOSEOUT-17` is closed and whether the remediation range
introduces a behavioral regression. Earlier passed implementation is not
reopened except where it participates in the benchmark's terminating backlog
decision. `FIND-TASK-008-CLOSEOUT-13` and the full default `bench:capacity`
qualification remain deferred to integration; this review makes no empirical
AC-040 or AC-041 claim.

## Caller-to-result trace

`OracleQueryAudit::stage` increments the process-owned pending count and
`audit_outbox_pending` gauge before enqueueing a decision
(`crates/wyrd/wyrd-server/src/oracle/query_audit.rs:101-116`). The writer
commits the canonical staging transaction and only then decrements the count
and gauge, or decrements after recording counted loss
(`query_audit.rs:155-215`). This agrees with the Bifrost authority: a read
decision is pending until commit or counted loss, and a committed staging row
remains owed until `AuditPublisher` advances its tenant watermark
(`architecture/bifrost-design.md:587-595,613-630`).

The changed owner, `Queue::poll`, now executes the empty-path observation in
the required order (`capacity/evidence.rs:332-352`):

1. scrape every replica (`S1`);
2. read the durable backlog (`Q1`) and return immediately if `S1 + Q1` is
   nonzero;
3. scrape every replica again (`S2`); and
4. read the durable backlog again (`Q2`) and return `S2 + Q2`.

The previously open schedule is therefore covered. If the decision is still
pending at `S2`, the returned audit cell is nonzero. If its commit crosses
`Q1` and releases the pending gauge before `S2`, the staging row is committed
before `Q2` begins and remains visible there until publication. Counted loss
may legitimately clear both owners. An empty `Q1` also establishes that every
expected step run exists and is terminal, so the capacity workload cannot
create a new step-owned audit producer after that boundary. The outer
`Deployment::drain` loop and its exact 60-second judgment remain unchanged
(`capacity/step.rs:383-423`).

The focused public Oracle proof exercises both sides of this boundary. It
retains the still-pending phase (`capacity/evidence.rs:656-723`) and adds the
missed transfer schedule (`evidence.rs:725-787`): `S1` captures pending zero,
the public query creates a decision while the chain-head transaction holds its
commit, `Q1` reads before commit, the `S2` callback releases the transaction and
waits for pending to reach zero, and `Q2` observes the committed unpublished
row. The assertions then prove the row remains owed above the watermark and
clears only after publication. This is the same public producer, process gauge,
canonical staging path, publisher, and terminal `Queue::poll` consumer used by
the benchmark, rather than a test-only substitute.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-R6-1 / FIND-17: a terminating poll cannot miss a decision whose commit crosses its first durable snapshot | `Queue::poll` takes `Q2` after `S2` and combines the latest scrape with the latest durable read (`evidence.rs:332-352`); the audit writer commits before decrementing pending (`query_audit.rs:155-215`) | Source-ordered producer-to-consumer trace; focused Oracle proof's crossed phase at `evidence.rs:725-787` | **PASS — CLOSED** |
| AC-R6-2: retain the still-pending case and directly prove the commit/decrement-before-`S2` case through the public Oracle path | Existing held-pending phase remains at `evidence.rs:656-723`; new phase forces `S1 = 0`, `Q1 = 0`, commit/decrement, `S2 = 0`, then requires `Q2 = 1`, persistence above watermark, and zero after publication | Test is present, selected, and compiled in the capacity target. Its repository-managed execution was attempted but could not access the Docker API; the implementation record reports the exact test passing and records the red failure without `Q2` | **PASS within stated environment limit** |
| AC-R6-3: preserve adjacent drain arithmetic, deadline behavior, workload/report/public contracts, test placement, and scheduling | Remediation source diff is limited to `Queue::poll`, its rustdoc, and the existing `pg_tests` proof; `Deployment::drain`, `Drain::judge`, workload, report, server producer, publisher, and nextest configuration are unchanged | Exact adjacent tests `pending_decisions_add_to_staged_audit_rows` and `a_backlog_drains_only_within_the_limit`: 2 passed; `git diff --check` clean | **PASS** |
| Closure non-goals: no production audit redesign, blocking commit, new durable owner, public contract, timeout, SLO, or full default qualification | No production code, schema, API, publisher, SLO, report, configuration, or scheduling file changed in the remediation range | Complete range inspection; implementation record explicitly leaves FIND-13 and default qualification deferred | **PASS** |
| User-directed regression boundary: no change in the range can make the capacity audit result falsely PASS or FAIL | Nonempty `S1 + Q1` keeps the established early return; only the formerly unsafe empty path adds `S2 -> Q2`; latest scrapes are returned with the latest combined backlog | Source trace through `Deployment::drain` and `Drain::judge`; adjacent focused tests pass | **PASS** |

## Verification evidence and limits

- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E
  'test(=evidence::tests::pending_decisions_add_to_staged_audit_rows) |
  test(=step::tests::a_backlog_drains_only_within_the_limit)'`: **2 passed**.
- The exact repository-managed Postgres selector for
  `evidence::pg_tests::the_audit_backlog_holds_from_a_pending_decision_until_its_publication`
  was attempted. The wrapper could not connect to the Docker API in this
  sandbox, so the test did not begin. The changed test nevertheless compiled
  as part of the capacity binary, and the remediation record reports the exact
  focused proof passing plus its expected red failure when `Q2` is removed.
- `git diff --check
  ea0ed46fad8aa9ffa38c29c1b2d093d9d06177e8..c4bc77a5c877c508191dc606b3cd3bb78047dc29`:
  clean.
- The complete unmodified default `mise run bench:capacity` remains deferred
  under `FIND-TASK-008-CLOSEOUT-13`. No AC-040/AC-041 qualification is inferred
  from the focused closure evidence.

## Proposed findings

None. The remediation closes the previously retained false-zero interval at
its existing `Queue::poll` decision owner, and the reviewed range introduces no
behavioral regression within the caller-defined boundary.

## Overall result

**PASS**

`FIND-TASK-008-CLOSEOUT-17` is closed. A terminating capacity poll now pairs
its final replica observation with a later durable observation, and the focused
proof forces the exact commit/decrement handoff that round seven could not
observe. The default capacity qualification remains deferred as directed.
