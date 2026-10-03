# TASK-008 closeout task-review verdict, round 5

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `0973a03e5a389ea0fb3635c6d9175e25db0a6da0`
- Cumulative range: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13..0973a03e5a389ea0fb3635c6d9175e25db0a6da0`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior reviews: `review/TASK-008-r1/` through `review/TASK-008-r4/`
- Reviewed remediation: `review/TASK-008-r4/TASK-008-CLOSEOUT-R3-process-boundary-and-proof.md`

The candidate remained unchanged throughout review. This checkout has no
`.codegraph/` directory, so navigation used Git, `rg`, and direct source
inspection. Only round-five review artifacts were written.

The caller's two fixed boundaries are preserved. The unmodified default
benchmark, `FIND-TASK-008-CLOSEOUT-13`, is deferred to integration after the
other workstreams merge and is not a candidate blocker or empirical evidence.
The report is intentionally written before Postgres teardown; the integrator-
rejected request to include post-report teardown in `Report::total_seconds` is
excluded and was not revived.

## Reconciled acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Review the approved revision-57 task and full cumulative candidate | Immutable spec, task, base, candidate, and complete changed-path inspection | Candidate identity checked before and after review | PASS |
| REQ-171 one capacity entry point, approved workload, step sequence, SLOs, report, and verdict | `mise.toml` `bench:capacity`; `wyrd-testing` `capacity` binary and modules | Capacity target passed 14 tests; focused load/report/evidence tests passed | PASS structurally |
| Accepted prior FIND-2 process boundary | One exported absolute deadline; non-foreground timeout process groups; finite TERM-to-KILL escalation; wrapper reserve | Both task-text descendant tests passed | PASS; prior finding closed |
| Prior FIND-14 async cleanup behavior | Replica stops cross Tokio's blocking boundary while preserving stop/reap/log semantics | Recorded compatible-host heartbeat proof; source trace; local rerun was blocked before the assertion by unavailable user systemd | PASS behaviorally |
| Prior FIND-15 exact evidence commands | Six complete pinned exact commands in the active task matrix | Each recorded passing; whole target passed | PASS |
| Required struct-centered ownership for materially changed Rust | `Benchmark::clean_up` delegates live replica-set shutdown and output handling to free async `stop_replicas` | Maintainer discovery, focused follow-up, and independent validation | **FAIL — `FIND-TASK-008-CLOSEOUT-16`** |
| REQ-171 audit-outbox backlog drains within 60 seconds | Drain combines replica metrics with committed `vala.audit_staging` rows | Producer-to-sink trace shows process-local pending is absent and post-stop commits are permanently excluded | **FAIL — `FIND-TASK-008-CLOSEOUT-17`** |
| AC-040/AC-041 empirical default qualification | No qualifying default execution is recorded | Explicit caller sequencing | DEFERRED as FIND-13; non-blocking |
| Preserve non-goals and accepted report boundary | No public API, workload, SLO, production timeout, storage format, or capacity claim changed; report remains pre-teardown | Complete diff and consumer inspection | PASS |

## Independent review results

| Report | Result | Material contribution |
|---|---|---|
| `task-review-behavior.md` | PASS | Closed accepted process, async-behavior, and exact-command hypotheses. |
| `task-review-invariants.md` | PASS | Traced deadline, process groups, cancellation, and sibling consumers. |
| `standards-review.md` | PASS | Found no standards issue; its characterization of the shutdown helper was later revised by validation. |
| `maintainer-review.md` | FAIL | Proposed the ownerless replica-shutdown workflow finding. |
| `system-review.md` | PASS | Found no remaining signal, crash, timeout, or recovery defect. |
| `domain-review-capacity.md` | PASS | Confirmed workload/report/SLO structure and the deferred empirical boundary. |
| `domain-review-process-lifecycle.md` | PASS | Confirmed descendant ownership, escalation, and wrapper teardown. |
| `domain-review-durability.md` | FAIL | Proposed the process-local audit handoff omission. |
| `followup-review.md` | RESOLVED | Resolved the ownership conflict and confirmed the audit handoff's reachability and correction boundary. |
| `findings-validation.md` | FIX_REQUIRED | Independently retained and deduplicated findings 16 and 17. |

All required reports are present.

## Follow-up decision

A focused follow-up was required because discovery conflicted on whether
`stop_replicas` was a permissible adapter and because the durability reviewer
found an unreviewed process-local audit path. The follow-up resolved both from
source: `spawn_blocking` is correct, but the surrounding collection workflow
has natural owners; and the shared audit outbox can remain pending after a
request returns, then commit after the drain's stop timestamp.

## Validated finding ledger

| ID | Status | Classification | Exact boundary | Required outcome |
|---|---|---|---|---|
| `FIND-TASK-008-CLOSEOUT-16` | CONFIRMED | VIOLATION | `capacity/main.rs:589-604,649-677`; `capacity/step.rs:38-56`; `release_server.rs:365-405` | Delete the ownerless free shutdown workflow and place replica-set orchestration on an existing owner while preserving the blocking boundary and all stop behavior. |
| `FIND-TASK-008-CLOSEOUT-17` | REVISED | INCORRECT | `capacity/evidence.rs:159-180,223-264`; `capacity/step.rs:320-361,383-420`; `oracle/query_audit.rs:48-121,142-243` | Bridge audit drain evidence from the existing process-local pending owner to every unpublished staged row, including post-stop commits. |

The complete diagnoses, source traces, correction decisions, and focused proof
requirements are preserved in `findings-validation.md` and packaged in
`TASK-008-CLOSEOUT-R4-audit-handoff-and-owner-shape.md`.

## Verification limits

- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity`:
  14 passed, 0 failed, 4 ignored environment/process tests skipped.
- Both exact task-script process-group tests passed: the pre-wrapper test in
  about 7 seconds and the nested-run test in about 30 seconds.
- The slow-replica heartbeat proof could not start its systemd scope in this
  sandbox (`Failed to connect to bus: Operation not permitted`); it did not
  reach the behavior under test. A compatible-host PASS is recorded and the
  `spawn_blocking` path is source-visible.
- Round-five reviewers recorded the six focused capacity tests, two
  `release_server` tests, formatting, and diff check as passing.
- No proof exercises process-local audit pending through `Deployment::drain`
  and the later commit-to-publication handoff.
- The full default benchmark remains expressly deferred under FIND-13 and
  provides no empirical AC-040/AC-041 qualification here.

## Prior-finding closure

Findings 1 through 12, 14, and 15 are closed. Finding 2 is closed for the
integrator-accepted process-boundary portion; its rejected post-report elapsed
sub-part remains excluded. Finding 13 remains deferred and non-blocking. New
finding 16 preserves the behavioral closure of finding 14 while correcting the
owner shape introduced by that remediation. New finding 17 concerns the
separate audit pending-to-staging evidence handoff.

## Verdict

**FIX_REQUIRED**

The accepted process-lifetime and async-worker remediations are behaviorally
closed, but the candidate retains one hard owner-shape violation and one
reachable audit-drain correctness gap. Both corrections are bounded within the
approved private benchmark, existing telemetry, and audit-outbox ownership;
neither requires a specification revision.
