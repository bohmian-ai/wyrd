# TASK-008 closeout task-review verdict, round 3

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `5c3bb79b3598abd88a3a234611fc400096adc975`
- Cumulative range: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13..5c3bb79b3598abd88a3a234611fc400096adc975`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review and remediation: `changes/active/verified-change-contract/review/TASK-008-r1/`
- Prior blocked review reused as source hypotheses: `changes/active/verified-change-contract/review/TASK-008-r2/`

The candidate commit remained unchanged throughout this review. CodeGraph is
not indexed in this checkout, so source, caller, and consumer tracing used Git
and repository navigation directly. Only review artifacts under
`review/TASK-008-r3/` were written.

## Reconciled acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Review the exact approved authority | Candidate `spec.md` and task frontmatter both name approved revision 57 | Immutable blob and metadata inspection | PASS |
| REQ-171: one server-capacity entry point | `bench:capacity` and the `capacity` binary remain; the three superseded capacity binaries/tasks are removed | Cumulative path and manifest inspection | PASS |
| REQ-171 workload, step sequence, knee, verdict, and golden-signal cells | `capacity/{main,load,step,evidence,report}.rs` implements the four-tenant mix, ramp/sustained/scale-out sequence, typed `StepKind`, backlog/drain/resource evidence, and one report table | Capacity binary target: 14 passed | PASS structurally |
| AC-040: non-judge overhead sample floor and separate judge diagnostics | Report logic applies the four non-judge thresholds and renders provider wait apart from judge engine overhead | Focused report/evidence tests in the capacity target | PASS structurally |
| AC-041: default-queue ingest evidence and test-owned durability | Benchmark records queue refusals/drain; the real-server Rust journey owns exactly-once rows and flat client bytes | Recorded focused journey evidence plus source trace | PASS structurally |
| Prior findings 1, 3-5, and 7-12 | Legacy entry point removed; provider wait added; task authority corrected; lifecycle and step identity have owners; staged backlog, exact drain edge, one table, common resource window, and test-owned duplication are present | Discovery coverage and Ponytail validation | PASS; closed |
| Prior FIND-2: the complete default command, including setup and cleanup, finishes under one enforceable 30-minute boundary | `Lifetime` begins inside the binary; wrapper setup/build precedes it; `Benchmark::prepare` is outside its timeout; migration/setup use blocking `Command::output`; replica stop/report finalization are outside the same enforced deadline | Cooperative paused-time unit test only; no stalled-child/process proof | **FAIL — `FIND-TASK-008-CLOSEOUT-2`** |
| Prior FIND-6: every materially changed async cancellation owner documents cancellation and partial progress | Caller-specific methods document cancellation, but shared `Lifetime::bounded` owns `timeout_at` and future drop without its own cancellation/partial-progress contract | Static async-item audit | **FAIL — `FIND-TASK-008-CLOSEOUT-6`** |
| Prior FIND-13: one unmodified default capacity execution | No qualifying default execution is recorded | Explicit caller sequencing | **DEFERRED** to integration after other workstreams merge; not a blocker for this candidate |
| Preserve non-goals and adjacent behavior | No public API, production request deadline, durable schema, or compatibility surface is added; correctness remains in focused tests | Complete cumulative diff and consumer inspection | PASS |

## Independent review results

| Report | Result | Material contribution |
|---|---|---|
| `task-review-behavior.md` | FAIL | Proposed the command-lifetime and cancellation-documentation findings. |
| `task-review-invariants.md` | FAIL | Traced the deadline value from the wrapper through blocking setup and cleanup consumers. |
| `standards-review.md` | FAIL | Confirmed the async blocking and mandatory rustdoc violations; other touched rules pass. |
| `maintainer-review.md` | FAIL | Confirmed the advertised lifecycle cannot be enforced and the shared cancellation owner is undocumented. |
| `system-review.md` | FAIL | Traced stalled-child, timeout, cleanup, reaping, logging, and final-report failure paths. |
| `domain-review-capacity.md` | FAIL | Confirmed workload/report semantics and isolated the command-wide elapsed-boundary defect. |
| `domain-review-durability.md` | FAIL | Confirmed durable backlog/drain behavior and the uncovered migration/setup child lifecycle. |
| `findings-validation.md` | FIX_REQUIRED | Independently validated and deduplicated the two stable findings below. |

All required reports are present.

## Follow-up decision

No `followup-review.md` was needed. The discovery reports materially agree,
their process-lifecycle observations share one source under prior FIND-2, and
they reveal no reachable path outside the union already traced. The structured
Ponytail reviewer independently validated this decision.

## Validated finding ledger

| ID | Status | Classification | Exact boundary | Required outcome |
|---|---|---|---|---|
| `FIND-TASK-008-CLOSEOUT-2` | REVISED, OPEN | INCORRECT | `mise.toml:507-524`; `capacity/main.rs:79-188,227-344,397-429,500-531,621-637`; `release_server.rs:130-189,347-388,469-483,618-635` | Put the complete default command process tree under one absolute setup-to-exit deadline and make migration/setup children terminable and reapable at expiry while preserving diagnostics and the existing failed-report path. |
| `FIND-TASK-008-CLOSEOUT-6` | CONFIRMED, OPEN | VIOLATION | `capacity/main.rs:174-188` | Document cooperative future drop, surviving effects, cleanup ownership, and workflow-specific retry safety on the shared cancellation owner or its direct replacement. |

The decision-complete evidence and correction boundaries are preserved in
`findings-validation.md` and packaged in the remediation task below.

## Verification limits

- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity`:
  14 passed, 0 failed in independent discovery runs.
- `git diff --check f6159606c5c959e8fcc3423574ab0e7e6c86ee13..5c3bb79b3598abd88a3a234611fc400096adc975`:
  passed.
- The existing lifetime test proves cooperative Tokio cancellation only. It
  does not exercise a stalled migration/setup child or the complete mise
  command.
- The unmodified default benchmark was not run. Per caller direction,
  `FIND-TASK-008-CLOSEOUT-13` is deferred to post-integration qualification and
  does not block this candidate. No empirical AC-040/AC-041 capacity claim is
  made by this review.

## Prior-finding closure

Prior findings 1, 3, 4, 5, and 7 through 12 are closed. Prior finding 2 is
revised and remains open. Prior finding 6 is confirmed and remains open. Prior
finding 13 is deferred by explicit caller sequencing and is not included in
this candidate's blocking ledger.

## Verdict

**FIX_REQUIRED**

The candidate closes the benchmark structure, evidence, report, and focused
correctness gaps, but it does not yet enforce the approved complete-command
30-minute boundary and leaves the shared cancellation contract undocumented.
Both are bounded implementation corrections within revision 57. See
`TASK-008-CLOSEOUT-R2-command-lifetime.md`.
