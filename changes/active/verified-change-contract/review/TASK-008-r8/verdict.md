# TASK-008 round-eight closure verdict

## Immutable subject

- Approved specification: `changes/active/verified-change-contract/spec.md`,
  revision 57.
- Original task:
  `changes/active/verified-change-contract/tasks/task-008-closeout.md`.
- Prior review:
  `changes/active/verified-change-contract/review/TASK-008-r7/`.
- Prior remediation:
  `changes/active/verified-change-contract/review/TASK-008-r7/TASK-008-CLOSEOUT-R6-close-final-audit-handoff.md`.
- Remediation base: `ea0ed46fad8aa9ffa38c29c1b2d093d9d06177e8`.
- Reviewed candidate: `c4bc77a5c877c508191dc606b3cd3bb78047dc29`.
- Reviewed range:
  `ea0ed46fad8aa9ffa38c29c1b2d093d9d06177e8..c4bc77a5c877c508191dc606b3cd3bb78047dc29`.

The candidate remained at the named commit throughout review. This closure
review was limited by user direction to `FIND-TASK-008-CLOSEOUT-17` and
regressions introduced by the remediation range. Earlier accepted code was
used only to trace a path capable of making the capacity result false.
`FIND-TASK-008-CLOSEOUT-13` and the complete default `bench:capacity` run
remain deferred to integration.

## Reconciled acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Close the `Q1 -> commit/decrement -> S2` false-zero interval from `FIND-TASK-008-CLOSEOUT-17` | `Queue::poll` preserves `S1 -> Q1` and, only on an empty first pair, returns a fresh `S2 -> Q2` pair (`capacity/evidence.rs:307-353`) | Producer-to-consumer trace through `OracleQueryAudit`, PostgreSQL staging, and `Deployment::drain`; public held-commit proof at `evidence.rs:725-787` | **PASS — FIND-17 CLOSED** |
| Prove the real public Oracle handoff remains nonzero until publication | The expanded ignored Postgres proof creates the decision after `S1`, holds commit through `Q1`, completes commit/decrement before `S2`, requires `Q2 = 1`, and publishes through the real owner | Implementation record reports the exact proof passing and the no-`Q2` red failure; review rerun could not start because Docker access was denied | **PASS with recorded environment limit** |
| Preserve adjacent benchmark behavior and introduce no executable regression | Nonempty early return, outer drain loop, exact deadline, workload, report, audit producer, publisher, test placement, and scheduling remain unchanged | Focused pure arithmetic and drain-edge selection passed 2/2; `fmt:check`, focused capacity Clippy, and range `git diff --check` passed | **PASS** |
| Preserve the approved task lifecycle in the changed remediation record | The range appends completed implementation evidence and hands the candidate to independent review | The header changes `status: ready` to undefined `status: implemented`; the authoritative next state is `review` | **FAIL — FIND-19** |
| Keep the full default capacity qualification deferred | No default benchmark result or AC-040/AC-041 qualification is claimed | User direction and the R6 record preserve the deferral | **PASS / DEFERRED** |

## Independent review results

| Review | Result | Reconciled conclusion |
|---|---|---|
| Behavior | PASS | FIND-17 closes; no executable regression. |
| Invariants | PASS | The final scrape is paired with a fresh durable statement. |
| Repository standards | FAIL | Proposed the undefined task-state violation. |
| Maintainer | PASS | The correction remains on the existing owner and the proof remains maintainable. |
| System resilience | PASS | The conditional read adds no retry, crash, or fabricated-result path. |
| Concurrency domain | PASS | The successful ownership-transfer schedule is continuously represented. |
| Durability domain | PASS | PostgreSQL visibility and watermark ownership close the handoff. |

The discovery reports materially conflicted only on the changed task status.
A fresh focused follow-up was therefore required. It resolved the conflict from
the closed lifecycle vocabulary and implementation-handoff contract: the
metadata change is an in-scope violation, and the complete correction is
`status: implemented` to `status: review`.

The fresh structured Ponytail validation independently confirmed FIND-17's
closure, confirmed the executable finding union as empty, and deduplicated the
standards and follow-up proposals into the single ledger entry below.

## Validated finding ledger

### `FIND-TASK-008-CLOSEOUT-19` — CONFIRMED — VIOLATION

- **Violated obligation:** `AGENTS.md` section 14 and
  `architecture/references/languages/spec-driven-development.md` require an
  implemented task submitted to independent review to use `status: review`.
  `IMPLEMENTED` is an execution result, not a task-front-matter state.
- **Location:**
  `changes/active/verified-change-contract/review/TASK-008-r7/TASK-008-CLOSEOUT-R6-close-final-audit-handoff.md:4`.
- **Evidence:** the reviewed range changes `status: ready` to
  `status: implemented`, appends completed implementation evidence, and hands
  the immutable candidate to this task review. `implemented` is absent from
  the authoritative lifecycle vocabulary.
- **Observable consequence:** task-selection, review, and completion consumers
  cannot classify the active remediation record under the repository's task
  contract.
- **Required correction:** change only the R6 header to `status: review`.
  Preserve the diagnosis, implementation evidence, executable correction,
  tests, and FIND-13 deferral. Prove the metadata-only correction by direct
  inspection and `git diff --check`.

## Prior-finding closure

| Finding | Result |
|---|---|
| `FIND-TASK-008-CLOSEOUT-17` | **CLOSED.** The terminal poll now pairs `S2` with `Q2`, and the public held-commit proof forces the exact formerly missed handoff through publication. |
| `FIND-TASK-008-CLOSEOUT-13` | **DEFERRED TO INTEGRATION.** It is non-blocking in this user-scoped closure review and supplies no AC-040/AC-041 qualification. |

## Verification limits

- The focused pure selection passed 2/2.
- `mise run fmt:check`, focused Clippy for the capacity binary, and range
  `git diff --check` passed.
- The repository-managed Postgres proof was attempted but did not start because
  the sandbox cannot access the configured Docker API. Its source ordering was
  independently validated, and the R6 implementation record reports the exact
  green run plus the expected red failure without `Q2`.
- The recorded ignored-inclusive capacity target, fixed-port pair,
  release-server tests, and server journey were not rerun in this review.
- The full default `mise run bench:capacity` remains deferred as directed.

## Verdict

**FIX_REQUIRED**

`FIND-TASK-008-CLOSEOUT-17` is closed and the executable remediation introduces
no validated regression. The range nevertheless introduces the bounded
metadata-only `FIND-TASK-008-CLOSEOUT-19`. The accompanying R7 remediation task
contains the complete one-line correction and static proof.
