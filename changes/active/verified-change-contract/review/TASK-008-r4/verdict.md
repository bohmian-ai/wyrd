# TASK-008 closeout task-review verdict, round 4

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `8022436387f3a9a9499527ebf8b8b8140c6559cb`
- Cumulative range: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13..8022436387f3a9a9499527ebf8b8b8140c6559cb`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior reviews: `review/TASK-008-r1/`, `review/TASK-008-r2/`, and `review/TASK-008-r3/`
- Reviewed remediation: `review/TASK-008-r3/TASK-008-CLOSEOUT-R2-command-lifetime.md`

The candidate remained unchanged throughout review. CodeGraph is not indexed
in this checkout, so navigation used Git, `rg`, and direct source inspection.
Only artifacts under `review/TASK-008-r4/` were written.

## Reconciled acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Review the exact approved authority | Candidate `spec.md` and task frontmatter identify approved revision 57 | Immutable blob and metadata inspection | PASS |
| REQ-171 benchmark workload, steps, SLOs, verdict, and report shape | `capacity/{main,load,step,evidence,report}.rs`, the `capacity` binary, and `bench:capacity` remain the single benchmark implementation | Focused capacity target passed 14 tests with one environment-gated test skipped | PASS structurally |
| AC-040 and AC-041 focused correctness and durability proof | Existing runtime and Rust SDK journeys remain in the cumulative candidate; no benchmark-only duplicate correctness owner was added | Recorded exact integration/journey evidence from the task and prior reviews | PASS structurally |
| Prior findings 1 and 3 through 12 | Consolidated entry point, typed step identity, complete backlog, exact drain edge, shared report schema/resource interval, separate judge wait, cohesive lifecycle owner, and cancellation documentation remain present | Discovery coverage and independent validation | PASS; closed |
| Prior FIND-2: one enforceable setup-to-exit deadline with matching elapsed evidence | One timestamp/deadline reaches `Lifetime`; `OperatorRun` owns direct migration/setup children | GNU `timeout` native contract/probe and source tracing show `--foreground` excludes descendants; `total_seconds` is sampled before the required command tail | **FAIL — `FIND-TASK-008-CLOSEOUT-2`** |
| Async runtime rule for ordinary replica cleanup | `Benchmark::clean_up` calls `LocalServer::stop` for each replica | Source trace shows up to 45 seconds of thread sleep plus blocking kill/wait/log copy on a Tokio worker | **FAIL — `FIND-TASK-008-CLOSEOUT-14`** |
| Current task evidence gives complete exact commands for named tests | Revision-57 implementation matrix names six focused capacity tests | Entries use raw `cargo nextest` or detached selectors rather than complete pinned commands | **FAIL — `FIND-TASK-008-CLOSEOUT-15`** |
| Prior FIND-13: one unmodified default capacity execution | No qualifying default execution is recorded | Explicit caller sequencing | **DEFERRED** to integration after other workstreams merge; not a blocker for this candidate |
| Preserve non-goals and adjacent behavior | No public API, production timeout, workload, SLO, storage format, concurrency policy, or capacity claim changed | Complete cumulative diff and consumer inspection | PASS |

## Independent review results

| Report | Result | Material contribution |
|---|---|---|
| `task-review-behavior.md` | FAIL | Continued the outer command-lifetime finding. |
| `task-review-invariants.md` | FAIL | Traced the exported deadline through foreground timeouts and descendants. |
| `standards-review.md` | FAIL | Proposed the blocking cleanup and current evidence-command violations. |
| `maintainer-review.md` | PASS | Found the in-binary owner, method, naming, and documentation shape maintainable. |
| `system-review.md` | FAIL | Traced descendant survival, teardown, reporting, and elapsed-tail behavior. |
| `domain-review-capacity.md` | FAIL | Confirmed the pre-Postgres TERM-only deadline gap. |
| `domain-review-process-lifecycle.md` | FAIL | Confirmed foreground timeout does not own the command process tree. |
| `followup-review.md` | RESOLVED | Resolved elapsed, async-blocking, and task-evidence uncertainties. |
| `findings-validation.md` | FIX_REQUIRED | Independently validated and deduplicated the three stable findings below. |

All required reports are present.

## Follow-up decision

A focused follow-up was required because discovery differed on whether the
reported elapsed interval, synchronous replica stop, and stale/current task
commands were distinct material findings. It resolved all three from approved
authority and source: elapsed fidelity remains part of prior FIND-2; normal
blocking replica stop is distinct; only the candidate-added current revision-57
evidence, not historical planning commands, violates exact-command rules.

## Validated finding ledger

| ID | Status | Classification | Exact boundary | Required outcome |
|---|---|---|---|---|
| `FIND-TASK-008-CLOSEOUT-2` | REVISED, OPEN | INCORRECT | `mise.toml:511-548`; Postgres wrapper teardown; `capacity/main.rs:392-424,702-728`; `capacity/report.rs:371-383` | Make every command phase own and terminate its complete descendant group inside the one absolute deadline, and finalize durable elapsed evidence after report and wrapper teardown. |
| `FIND-TASK-008-CLOSEOUT-14` | CONFIRMED, OPEN | VIOLATION | `capacity/main.rs:581-613`; `release_server.rs:365-405` | Run ordinary synchronous replica stop/reap/log-copy work through an explicit blocking boundary without changing stop semantics. |
| `FIND-TASK-008-CLOSEOUT-15` | CONFIRMED, OPEN | VIOLATION | `tasks/task-008-closeout.md:1498-1504` | Give each current named capacity test its complete pinned exact command and recorded result. |

The validated diagnoses, minimum correction boundaries, and focused closure
proof are preserved in `findings-validation.md` and packaged in the remediation
task below.

## Verification limits

- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity`:
  14 passed, 0 failed, 1 environment-gated test skipped.
- `git diff --check f6159606c5c959e8fcc3423574ab0e7e6c86ee13..8022436387f3a9a9499527ebf8b8b8140c6559cb`:
  passed.
- A native GNU `timeout --foreground` probe confirmed that TERM-ignoring work
  can run beyond the nominal timeout without `--kill-after`; native help also
  states that foreground mode does not time out children.
- Recorded R2 evidence includes the explicitly selected stalled-setup proof,
  release-server tests, formatting, and lints. That proof starts below the
  mise/wrapper/Cargo process-tree boundary.
- The unmodified default benchmark was not run. Per caller direction,
  `FIND-TASK-008-CLOSEOUT-13` is deferred and does not block this candidate.
  No empirical AC-040/AC-041 capacity claim is made here.

## Prior-finding closure

Prior findings 1 and 3 through 12 are closed. Prior finding 2 is revised and
remains open. Prior finding 6 is closed by the shared cancellation and
partial-progress documentation. Findings 14 and 15 are new bounded violations.
Finding 13 is deferred by explicit caller sequencing and is not part of the
blocking candidate ledger.

## Verdict

**FIX_REQUIRED**

The candidate closes the direct migration/setup child ownership and
cancellation-documentation gaps, but the actual command boundary still does
not own descendants, the reported total ends before the required command tail,
normal cleanup blocks a Tokio worker, and the current focused-test evidence is
not reproducible from complete pinned commands. These are bounded corrections
within approved revision 57. See
`TASK-008-CLOSEOUT-R3-process-boundary-and-proof.md`.
