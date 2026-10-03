# TASK-008 round-seven closure verdict

## Immutable subject

- Approved specification:
  `changes/active/verified-change-contract/spec.md`, revision 57 (`approved`).
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

The candidate remained at the named commit throughout review. CodeGraph had
no usable index for this checkout, so review used Git, `rg`, and direct source
inspection. Per caller direction, this closure review decides only whether
`FIND-TASK-008-CLOSEOUT-17` and `FIND-TASK-008-CLOSEOUT-18` are closed and
whether the remediation range introduces a regression. Earlier accepted code
was considered only where its interaction with the range can make the
benchmark report a false PASS or FAIL. `FIND-TASK-008-CLOSEOUT-13` remains
deferred to integration and supplies no empirical capacity qualification.

## Reconciled acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-17: a terminating drain poll cannot miss a decision created after its first replica scrape | `Queue::poll` now performs a first scrape (`S1`), durable snapshot (`Q1`), and, only for an otherwise-empty result, a second scrape (`S2`); `Deployment::drain` uses that result | The public Oracle held-commit proof creates the decision after `S1` and observes it pending at `S2` | **PARTIAL** |
| FIND-17: the same poll cannot miss the pending-to-staging handoff | The successful audit transition is pending -> staging commit -> pending decrement; `Queue::poll` combines `S2` with the already captured `Q1` and performs no durable read after `S2` | The proof holds the commit through `S2`, so it excludes `Q1 = 0 -> commit/decrement -> S2 = 0` | **FAIL — OPEN** |
| FIND-18: environment-owned proof uses the required test boundary | Live-server/Postgres helpers and proof are under `#[cfg(test)] mod pg_tests`; pure arithmetic and percentile tests remain under `mod tests`; the ignored environment gate remains | Source inspection and default capacity selection | **PASS — CLOSED** |
| Fixed-port capacity proofs do not race each other | `.config/nextest.toml` places exactly the two tests that start the replica-zero stand-in on port 8080 in a one-thread default-profile group | Static selector trace; the local two-test attempt was scheduled sequentially, but both tests were blocked at systemd setup by sandbox permission | **PASS** |
| Range introduces no other regression | Workload, SLOs, report, deadline, audit producer, publisher, and process lifecycle are unchanged | Focused unit tests, default capacity target, format, lints, diff check, and independent source audits | **PASS within verification limits** |

## Independent review results

| Review | Result | Reconciliation |
|---|---|---|
| Behavior | PASS | Revised: its producer and test-placement traces stand, but it did not cover a commit after `Q1` and before `S2`. |
| Invariants | FAIL | Confirmed under prior FIND-17. |
| Repository standards | PASS | No separate standards regression; its FIND-17 closure conclusion is revised by the durable handoff trace. |
| Maintainer | PASS | No maintainability finding; its closure conclusion is revised on correctness. |
| System resilience | FAIL | Confirmed under prior FIND-17. |
| Concurrency domain | PASS | Revised: it omitted the commit-before-decrement transition between `Q1` and `S2`. |
| Durability domain | FAIL | Confirmed under prior FIND-17. |

## Follow-up decision

A focused follow-up was required because four discovery reports accepted the
`S1 -> Q1 -> S2` bracket while three identified a missed ownership handoff.
The follow-up traced the Oracle producer, audit writer, PostgreSQL statement
snapshot, pending-gauge decrement, and terminal drain consumer. It resolved
the conflict in favor of the invariant, system, and durability analyses: a
commit after `Q1` fixes its snapshot can decrement pending before `S2`, making
both retained observations zero while staging owns an unpublished row.

The caller-approved public Oracle read is the correct proof surface; the gap
is its timing. The current proof holds the commit through `S2` and therefore
proves only the still-pending half of the false-empty interval.

## Validated finding ledger

### `FIND-TASK-008-CLOSEOUT-17` — REVISED — INCORRECT

Revision-57 REQ-171 requires every step-caused audit backlog to drain within
60 seconds before a step passes. `Queue::poll` at
`crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:324-341` performs
`S1 -> Q1 -> S2` and reuses `Q1` after `S2`. A decision produced after `S1`
can remain process-pending while `Q1` observes a terminal run and no committed
row, commit after that statement snapshot, and decrement pending before `S2`.
The poll then returns empty even though `vala.audit_staging` contains an
unpublished row. `Deployment::drain` can hand that false zero directly to
`Drain::judge`, causing a false PASS.

The existing public Oracle proof at `capacity/evidence.rs:637-671` holds the
commit until after `Queue::poll` returns, so it exercises
`S1 = 0, Q1 = 0, S2 = 1`, not the missed handoff.

Correction: preserve the existing audit owner, gauge, `S1`, `Q1`, `S2`,
non-blocking audit semantics, canonical staging, publisher, drain owner, and
deadline. Only when `S1 + Q1` is empty and `S2` is zero, reuse
`Queue::backlog` for a fresh durable observation after `S2` and combine that
fresh value with `S2` before accepting zero. Extend the held-commit Oracle
proof to release and finish the commit after `Q1` but before the terminating
pending observation, then prove the fresh durable row remains nonzero until
publication advances the watermark.

No new finding survived validation.

## Prior-finding closure

| Finding | Result |
|---|---|
| `FIND-TASK-008-CLOSEOUT-13` | **DEFERRED** to integration by caller direction; non-blocking here. |
| `FIND-TASK-008-CLOSEOUT-17` | **OPEN.** The range catches later work that remains pending through `S2`, but misses a commit crossing `Q1` before `S2`. |
| `FIND-TASK-008-CLOSEOUT-18` | **CLOSED.** Environment-owned proof code is under `pg_tests`; pure tests and the environment gate remain correctly placed. |

## Verification limits

- Focused pure tests for pending-plus-staged arithmetic and the exact drain
  edge: 2 passed.
- Default `wyrd-testing` capacity target: 15 passed, 5 environment tests
  skipped.
- `mise run lints`: passed.
- `cargo fmt --all -- --check` through `mise exec`: passed.
- `git diff --check 345295d8e..54373c36856d41ffe3554c977c35e9d033918cdd`:
  passed.
- The focused Postgres proof could not start because this sandbox cannot
  access the Docker API. The R5 implementation record reports it passing, but
  its current schedule does not exercise the validated handoff.
- The two fixed-port tests were selected together under the default profile
  and ran sequentially; both were blocked during systemd-scoped preparation by
  `Operation not permitted`, before their asserted behavior.
- The R5 record reports three complete ignored-capacity-target passes and the
  relevant server journey. Those green runs do not force the retained
  `Q1 -> commit/decrement -> S2` schedule.
- The complete unmodified `mise run bench:capacity` remains deferred as
  FIND-13.

## Verdict

**FIX_REQUIRED**

`FIND-TASK-008-CLOSEOUT-18` is closed and the fixed-port nextest group is a
valid, scoped regression correction. `FIND-TASK-008-CLOSEOUT-17` remains open
because the terminating poll can still miss a successful pending-to-durable
handoff. The correction is bounded within the existing `Queue::poll` owner and
public Oracle proof harness and requires no specification or architecture
revision.
