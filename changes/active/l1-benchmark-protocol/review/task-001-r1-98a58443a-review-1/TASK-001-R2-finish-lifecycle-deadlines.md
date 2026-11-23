---
id: TASK-001-R2
kind: remediation
status: proposed
spec: SPEC-l1-benchmark-protocol
spec_revision: 2
requirements: [REQ-002, REQ-003, REQ-005, AC-002, AC-003]
depends_on: []
parent_task: TASK-001-R1
remediates: [FIND-TASK-001-2, FIND-TASK-001-3]
---

# Give teardown one deadline across the case lifecycle

## Outcome and reviewed subject

A case whose actual phases fit their allowances must finish even when process
teardown crosses 510 seconds before its record exists. A case whose teardown
allowance expires must not authorize another launch through later cleanup
confirmation. Fix their shared cause: the child runner and supervisor do not
share an authoritative teardown transition and deadline. Preserve the four
closed findings and approved target removal.

Authority: [approved spec revision 2](../../spec.md). Original
[TASK-001](../../tasks/TASK-001-standardize-l1-benchmark-sweeps.md) and
[TASK-001-R1 with evidence](../task-001-1bcc403b-review-1/TASK-001-R1-l1-protocol-boundaries.md)
remain context. Reviewed candidate: `98a58443afac1f74cfefd607f3cd429c3b1cdee1`;
prior fully reviewed candidate: `1bcc403bf2c81c3cfa2c78a78f114cb9c5a49aaf`;
original base: `2418dc7848375c00ac6a991cafca88f722bcfdfe`.
See [validated findings](findings-validation.md) and [verdict](verdict.md).

## Root cause and correction boundary

The runner knows when process teardown actually starts, but the supervisor
learns about it from a final engine record written after process teardown.
The supervisor therefore mistakes a missing result for unfinished workload
execution at 510 s. Its timeout path then constructs a new teardown deadline
from the time it sends a stop signal, forgetting teardown already spent.
Both failures come from reconstructing the same lifecycle independently at
different stages. Longer timeouts or another final-record exception would
leave that cause in place.

Make the actual transition into teardown available to the supervisor before
the runner starts stopping processes. Establish one teardown deadline from
that transition, using the existing case timing and capped by the case's
pre-persistence envelope. Carry that deadline through SDK/provider/server
stop, PostgreSQL wrapper disposal, fallback termination and cleanup
confirmation. Passing between these operations, writing a final record,
receiving an interrupt or retrying cleanup must not renew the allowance.

Keep the existing `Lifecycle` phase accounting and `Sweep` supervision as the
owners. Extend their existing boundary with the minimum transition handoff
needed; leave its transport and private implementation to the implementer.
The supervisor must observe the transition rather than infer it from final
record availability. If failure or interruption starts teardown before the
normal runner transition, establish the same shared deadline once at that
point, within the existing total envelope. Missing final evidence never
proves timely cleanup.

The two retained findings are closure cases for this one correction:

- **FIND-TASK-001-2 / REQ-002/003:** `capacity/sweep.rs:303–336` extends
  execution past 510 s only if a final engine record already exists, but
  `run_case` writes it after `experiment::run` finishes process teardown.
  A case with 500 s pre-teardown work, 20 s process stop and 10 s PostgreSQL
  disposal fits every allowance and completes at 530 s, yet is stopped at
  510 s without its record. The teardown transition must reach supervision
  before this stop begins; its final record must not control whether the
  remaining teardown allowance exists.
- **FIND-TASK-001-3 / REQ-002/003/005:** after an admitted wrapper teardown
  expires, `sweep.rs:347–349,382–388` computes fresh stop/confirmation time
  instead of retaining the already-consumed teardown deadline. A case whose
  teardown expires at 550 s can clean up at 555 s within that renewed interval,
  receive `cleanup.confirmed = true`, and start the next case despite late
  confirmation. Stop and confirmation must consume the deadline established
  at the original transition, including process and wrapper time already
  spent. Preserve evidence of later physical cleanup separately from whether
  cleanup was confirmed within allowance.

Reuse `Sweep`, existing child runner/workload phase ownership, release-server
and PostgreSQL cleanup mechanisms. Remove or replace the final-record-based
phase inference and the fresh-deadline calculations they duplicate; do not
leave them as alternate timing authorities. Inputs are the actual runner
phase transition and existing case allowances; consumers are process/cleanup
waits, persisted lifecycle evidence and next-case admission. No new lifecycle
framework, production behavior or extra grace is required.

## Observable closure scenarios

1. **Allowed process teardown before the engine record exists** —
   FIND-TASK-001-2; REQ-002/003, AC-003. In the existing runner/supervisor
   tests, use controlled time for 240 s setup, 30 s warmup, 90 s measurement
   and 140 s drain. Start a 20 s process stop at 500 s; write the engine record
   only after that stop at 520 s, then complete PostgreSQL cleanup by 530 s.
   Exercise the actual transition handoff before process stop, with no final
   record available at 510 s. Assert the deadline established at 500 s remains
   560 s through process stop, record creation, wrapper disposal and cleanup
   confirmation; the case completes at 530 s and retains a valid saved
   outcome. Also drive a genuine phase overrun and assert INVALID with bounded
   cleanup. A prewritten record alone does not prove this path. Failure is
   early rejection, a missed handoff, or renewing the deadline to accept an
   overrun.

   **Evidence (PASS; TDD PASS).** Uncommitted working tree on `98a58443a`.
   - **Handoff and owner reuse.** `Lifecycle` stays the owner. `Lifecycle::announce_teardown` / `teardown_began` (`experiment.rs:2210,2220`) write and read `<artifacts>/teardown`, which holds the case-clock second teardown began, written by atomic rename. The artifacts directory is already the runner-to-supervisor channel (`timings/`), so no new transport or option was added. `run` writes the file right before `workload.finish()` (`experiment.rs:2705`). A failed write makes the case INVALID (`evidence.missing`).
   - **Supervisor.** `Sweep::attempt` polls for the announcement every `POLL` (`sweep.rs:352`). Teardown begins once, at the first of: the announcement (clamped to now), the 510 s cutoff, an interrupt, or process exit. The cutoff, interrupt and exit paths all read the announcement before falling back to their own instant, so an announcement made between two polls is never replaced by a later time. Its deadline is `min(began + 60 s, 570 s)`. The final-record inference (`teardown_left`, `child_teardown`, `confirmed_by`, the renewed stop time) is deleted. `lifecycle.teardown_seconds` and `cleanup.seconds` are both measured from when teardown began to the end of confirmation.
   - **Runner transition and record ordering.** `experiment::tests::a_case_announces_teardown_before_it_stops_its_processes` (`experiment.rs:7558`):
     - Its workload's `finish()` reads the announcement before stopping, at 144 s on the case clock (20 s spent before the runner + 30 warmup + 90 measured + 4 drains).
     - The stop is still charged to teardown (10 s), and the record is returned only after the stop.
     - RED before `run` announced: `teardown was announced before the stop began` (`expect` on `None`).
   - **Supervisor, allowed teardown.** `sweep::tests::an_announced_teardown_runs_past_the_cutoff_before_its_record` (`sweep.rs:1135`):
     - The fake wrapper announces 500 s, writes the record only after its 520 s gate, and exits at 530 s. No record exists at 510 s.
     - Asserts VALID, return at 530.0 s, cleanup confirmed, and `cleanup.seconds` and `teardown_seconds` both 30.0, replacing the runner's 20 s.
     - RED: the case was stopped at the 510 s cutoff, so the record gate was never passed (`nothing passed …/record.passed`).
   - **Genuine overrun.** `sweep::tests::a_stop_past_its_teardown_deadline_is_stopped_at_it` (`sweep.rs:1185`):
     - Teardown is announced at 500 s and the stop never finishes.
     - The case is stopped at exactly 560.0 s: INVALID with `…from 500.0s to 560.0s`, `kept no engine record`, and cleanup unconfirmed `…ending at 560.0s`.
     - `cleanup.seconds` is 60.0, and the next case is NOT RUN (launched 1).
     - RED: launched 2, because the old cutoff path confirmed the cleanup and admitted the next case.
   - **Test clock.** `stepped` (`sweep.rs:1059`) runs the real `Sweep::attempt` with real subprocesses on a paused clock. A running `spawn_blocking` holds tokio's auto-advance, so only `tokio::time::advance` moves time, and each wrapper step is gated on a file. Results are deterministic: 8 repeated runs of the sweep group all passed in about 0.2 s.
   - **Mechanics.** The scenario tests are `#[test]` functions on a built paused runtime, because `engine_record` starts its own runtime. `a_recorded_case_keeps_its_teardown_allowance_past_the_cutoff` and `an_admitted_wrapper_teardown_is_charged_to_teardown` used the prewritten-record inference this task removes, so they were replaced:
     - their "teardown did not finish within its 60s allowance" assertion is kept in the overrun tests;
     - their "wrapper disposal is charged to teardown" assertion is kept in the allowed case (30 s).

     The hung-case test now expects `did not begin teardown within its 510s deadline` instead of `kept no record…`; it failed RED on that wording first.
2. **Teardown expiry cannot renew admission authority** — FIND-TASK-001-3;
   REQ-002/003/005, AC-002. Through the existing supervisor cleanup path,
   complete pre-teardown work at 490 s, process stop/record creation at 500 s,
   and let the wrapper remain alive until its shared teardown deadline at
   550 s. Make it exit and resource checks return empty at 555 s, during what
   would previously be renewed grace. Assert saved INVALID, cleanup not
   confirmed within allowance, and later selected cases NOT RUN with no
   launch. Assert the same deadline established at the 490 s teardown
   transition governs process stop, wrapper wait, termination and confirmation;
   no stage replaces 550 s with a later expiry. Retain an in-budget counterpart
   proving that timely confirmed cleanup still permits continuation after an
   invalid case. Failure is renewed grace or treating late physical absence as
   timely cleanup confirmation.

   **Evidence (PASS; TDD PASS).** Same change and handoff as scenario 1.
   - **Single deadline.** `stop` and `confirm` take `within(deadline)`: the teardown deadline while it lasts, and after it has passed only the 570 s envelope, used for physical cleanup. Confirmation is `surviving.is_empty() && done < deadline` (`sweep.rs:401`), so cleanup that finishes late is kept as evidence (`surviving` empty, `seconds`, `it finished at …s`) but never confirms.
   - **Expiry.** `sweep::tests::an_expired_teardown_cannot_confirm_later_cleanup` (`sweep.rs:1237`):
     - Teardown is announced at 490 s, the record is written at 500 s, and the wrapper is still alive at 550 s. It is TERMed at 550 s and its trap exits at 555 s, and Docker then reports nothing left.
     - Asserts a return at 555.0 s, INVALID, `cleanup.confirmed == false` with `surviving` empty, and `cleanup.seconds` 65.0.
     - Asserts the reasons `…from 490.0s to 550.0s`, `…allowance ending at 550.0s: it finished at 555.0s` and `teardown took 65.0s, over its 60s allowance`.
     - The second case is NOT RUN with the could-not-be-confirmed reason (launched 1).
     - RED: `exit.passed` never appeared, because the old code admitted a record-derived window and did not stop the wrapper at 550 s.
   - **In-budget counterpart.** `sweep::tests::timely_cleanup_after_an_invalid_case_starts_the_next` (`sweep.rs:1303`): two cases are INVALID for a 160 s drain overrun with cleanup confirmed. Both launch, so an invalid case with timely cleanup still lets the sweep continue. This is retained behavior: it passed before and after the change, so no RED applies.
   - **Late or missing Docker answers.** `sweep::tests::late_or_missing_cleanup_answers_stop_the_sweep` (`sweep.rs:1489`) moved to the stepped clock, because the old fixture relied on the record-based 59.5 s accounting:
     - The wrapper is gone at 559.5 s, and the Docker check is pending when the clock passes 560 s, with a 2 s or 600 s delay.
     - Both delays give unconfirmed cleanup with `did not answer within its allowance` and the next case NOT RUN.
     - RED: launched 2.
   - **Retained assertions.** The hung case and hung Docker (`sweep.rs:1436`, cutoff 510 s, deadline 570 s, `cleanup.seconds ≤ 60.5`), interruption, saved evidence (`sweep.rs:1341`), process stop and drain (`experiment::tests::case_stops_spend_teardown_not_unused_drain`) and the connected smoke all still pass unchanged.

**Commands (serialized; final source).**
- `mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::a_case_announces_teardown_before_it_stops_its_processes)'`: RED as above, then GREEN.
- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(/^sweep::tests::/)'`: RED as above (6 failed, 4 passed), then GREEN with 10 passed, repeated 8 times.
- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity`: 112 passed, 3 skipped.
- `mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(/^experiment::/)'`: 27 passed.
- `mise run fmt:check`, `mise run lints` and `git diff --check`: each exited 0.
- `mise run test:benchmark:l1`: 1 passed in 221 s (`tests::pg_tests::l1_sweeps_isolate_and_record_cases`, real runner announcement against real Docker and PostgreSQL).

**Limits.** Full-size sweeps were not run (AC-005 is held). The test clock proves the deadline arithmetic and ordering, not real host timings. Allowances (590 s total), the 510 s cutoff and the 570 s envelope are unchanged. No grace was added, and no targets or L2 policy changed. Only `experiment.rs`, `bin/capacity/sweep.rs` and `bin/capacity/summary.rs` (rustdoc on `Cleanup`) changed.

Use controlled-clock and owned local subprocess proof in the existing inline
`capacity/sweep.rs` tests, connected to the actual runner transition and
record-write ordering in scenario 1. Do not substitute unrelated timing
fixtures for the producer-to-supervisor handoff. Retain existing process-stop,
drain, interruption, missing-record/hung-case, hung-Docker, late-Docker,
saved-evidence and connected smoke assertions. New test names and
private mechanics remain implementer-owned; confirm actual names before
recording exact focused commands and observed results in these entries.

Run focused tests with the repository runner, for example
`mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=<confirmed-test-name>)'`.
If runner behavior changes, use the corresponding `--lib` focused command.
Use default features and repository setup where required, serialize Cargo
work, and run `mise run fmt:check`, `mise run lints` and `git diff --check`
after the affected owner checks. Run `mise run test:benchmark:l1` for the
connected lifecycle regression boundary; no full journey aggregate or
full-size performance sweep is required by this bounded remediation.

## Preserved behavior and exclusions

Keep build-free single-registry listing, measured persistence duration,
retained full snapshots, ordinary/profiled no-target output, smoke integrity,
fresh resources and historical record interpretation. All old performance
targets remain void; no replacements or L2 policy changes belong here.
Keep the approved phase allowances and the 590 s total. Full-size feasibility
remains unverified while AC-005 execution is held.

Historical TDD gaps create no repair work. Verify the behavioral corrections
without manufacturing historical failures or rewriting already-correct code.
No commit or push is authorized by this review artifact.
