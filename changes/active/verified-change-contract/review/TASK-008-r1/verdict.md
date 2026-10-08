# TASK-008 closeout task-review verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd`
- Candidate: `852894689388124960993014a46934e73c0ed2a8`
- Cumulative diff: `ce5c09ef3..852894689`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 57, REQ-171, AC-040, AC-041, and the revision 57 history entry
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`, including `bench:capacity implementation (revision 57)`
- Pre-approved cumulative harness commit: `f6159606c`

The candidate commit remained unchanged throughout review. Only this review
directory was added to the working tree.

## Reconciled acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-171 exposes one `bench:capacity` server benchmark and retires every other server-capacity entry point | Consolidated `capacity` binary exists, but `bench:bifrost:query-capacity`, its Cargo target, and source remain reachable | Source and task-list inspection | **FAIL** — FIND-1 |
| REQ-171 runs the exact four-tenant public-client mix | `capacity/load.rs::mix`, request paths, and fixture owners | Capacity unit tests passed; source trace | PASS |
| REQ-171 runs warmup, ramp, sustained, and scale-out steps | `capacity/main.rs` and `Deployment::run` | Reduced smoke traversed the topology | PASS |
| REQ-171 default command completes within 30 minutes including setup | Elapsed time is recorded, but no lifecycle deadline bounds setup, request tails, or cleanup | No default run; only shortened smoke | **FAIL** — FIND-2, FIND-13 |
| REQ-171 judges traffic, errors, overhead, ingest drain, and every required backlog | Core cells exist, but Scribe staged work is omitted, the 60-second edge can pass late, and duplicate-run correctness is added beyond the approved SLO | Unit tests do not cover these gaps | **FAIL** — FIND-8, FIND-9, FIND-12 |
| REQ-171 reports one shared-column table with step rows followed by operation rows | Renderer emits two differently shaped tables | Source inspection | **FAIL** — FIND-10 |
| REQ-171 reports comparable per-step CPU and peak memory | CPU and memory are sampled over different intervals; CPU uses the planned rather than measured denominator | Source inspection | **FAIL** — FIND-11 |
| AC-040 uses the exact five reference workloads and enforces non-judge p95/sample requirements | Fixtures and overhead cells match the prescribed workloads and correctly fail when evidence is short | Unit/source proof; no authoritative default run | PASS structurally; empirical proof missing under FIND-13 |
| AC-040 reports judge overhead and provider waits separately | Judge overhead and call count exist; provider-wait duration does not | Source inspection | **FAIL** — FIND-3 |
| AC-041 offers 500 observations/s × 100 rows at `L = 200` through default queue and judges refusal/drain | Load arithmetic and default queue path are present | Unit test; shortened smoke never reached `L = 200` | PASS structurally; empirical proof missing under FIND-13 |
| AC-041 real-server tests prove exactly-once 100-row observations and flat client bytes | Sustained Rust journey reads durable rows and samples queue-owned bytes | Focused journey passed | PASS |
| Revision 57 moves fairness, claim exactly-once, and judgment correctness to tests without removing explicit request-error accounting | Focused fairness/claim/judgment tests exist; benchmark still fails on a duplicate-run assertion not named by the Errors SLI | Focused tests passed; source inspection | **FAIL** — FIND-12 |
| Repository task, ownership, and documentation rules remain satisfied | Task frontmatter is stale; lifecycle/reconnection ownership is split across free functions; closed step kind is stringly typed; async effect docs omit cancellation/partial progress | Standards and maintainer audits | **FAIL** — FIND-4 through FIND-7 |
| Non-goals: no production runtime redesign, public API change, new limiter, storage format, or relaxed auth/tenancy/durability | Candidate changes benchmark/test support and focused tests; queue production behavior is unchanged | Cumulative diff inspection | PASS |

## Independent review results

| Report | Result | Material contribution |
|---|---|---|
| `task-review-behavior.md` | FAIL | Single-benchmark violation, lifetime bound, and missing full-run proof |
| `task-review-invariants.md` | FAIL | Single-benchmark violation, missing judge-wait report, and lifetime bound |
| `standards-review.md` | FAIL | Stale task contract, lifecycle ownership, and async documentation |
| `maintainer-review.md` | FAIL | State-owning free workflow and stringly verdict step identity |
| `system-review.md` | FAIL | Incomplete Scribe backlog and unbounded command lifetime |
| `domain-review-capacity.md` | FAIL | Deadline edge, report shape, resource window, legacy benchmark, and empirical proof |
| `domain-review-durability.md` | FAIL | Proposed correctness/durability scope drift, narrowed by follow-up |

## Follow-up decision

`followup-review.md` was required because discovery reports conflicted on
benchmark-owned correctness accounting and the `K = 50` sample floor. It
resolved all uncertainties:

- wrong judgments and lost requests remain required errors, while duplicate-run
  judgment is test-owned drift;
- `K = 50` may legitimately fail the 1,000-sample floor, so BHV-003 was
  rejected and no specification change is needed; and
- REQ-171 requires one shared-column report table, so CAP-002 remains.

## Validated finding ledger

`findings-validation.md` independently validated every proposal and retained
these bounded findings:

| ID | Status | Class | Location | One-line evidence |
|---|---|---|---|---|
| FIND-TASK-008-CLOSEOUT-1 | CONFIRMED | VIOLATION | `mise.toml:507-509`; `wyrd-testing/Cargo.toml:145-151`; `bifrost_query_capacity/main.rs:1-14` | A second release-server capacity command, target, and binary remain runnable. |
| FIND-TASK-008-CLOSEOUT-2 | REVISED | MISSING | `capacity/main.rs:113-279`; `fixture.rs:57-58,209-292`; `load.rs:249-307` | No absolute command deadline covers setup, issued-request tails, and cleanup. |
| FIND-TASK-008-CLOSEOUT-3 | CONFIRMED | MISSING | `capacity/evidence.rs:62-86`; `step.rs:157,220`; `report.rs:184-210,445-448` | Judge provider-wait duration is absent from Markdown and JSON evidence. |
| FIND-TASK-008-CLOSEOUT-4 | CONFIRMED | VIOLATION | `task-008-closeout.md:1-10,1485-1571` | Frontmatter still says proposed revision 49/SPEC_REVISION_REQUIRED while the task claims revision 57 implemented. |
| FIND-TASK-008-CLOSEOUT-5 | REVISED | VIOLATION | `capacity/main.rs:113-300`; `capacity/step.rs:33-50` | Dependency-backed lifecycle and deployment-mutating reconnection remain free workflows. |
| FIND-TASK-008-CLOSEOUT-6 | CONFIRMED | VIOLATION | `capacity/main.rs:106-113`; `fixture.rs:203-209`; `load.rs:240-249`; `step.rs:138-142` | Changed effectful async workflows omit cancellation and partial-progress contracts. |
| FIND-TASK-008-CLOSEOUT-7 | CONFIRMED | VIOLATION | `capacity/step.rs:55-66`; `report.rs:372-385`; `main.rs:188-245` | Verdict-critical closed step kinds are unchecked strings. |
| FIND-TASK-008-CLOSEOUT-8 | REVISED | INCORRECT | `capacity/evidence.rs:89-99`; `capacity/step.rs:244-258` | Scribe can appear empty while `bifrost_scribe_staging_live_members` remains nonzero. |
| FIND-TASK-008-CLOSEOUT-9 | CONFIRMED | INCORRECT | `capacity/step.rs:173-195,225-262`; `report.rs:221-237` | An empty observation after 60 seconds returns `Some` and is judged PASS. |
| FIND-TASK-008-CLOSEOUT-10 | CONFIRMED | INCORRECT | `capacity/report.rs:417-477` | The Markdown closes a step table and opens a differently headed operation table. |
| FIND-TASK-008-CLOSEOUT-11 | CONFIRMED | INCORRECT | `capacity/step.rs:89-98,143-205` | CPU and memory cover different intervals, and CPU divides by the wrong duration. |
| FIND-TASK-008-CLOSEOUT-12 | CONFIRMED | DRIFT | `capacity/step.rs:310-312`; `capacity/report.rs:4-8,447` | Duplicate-run correctness still fails capacity although revision 57 assigns it to tests. |
| FIND-TASK-008-CLOSEOUT-13 | CONFIRMED | MISSING | `task-008-closeout.md:1493-1501,1535-1563` | Only an intentionally failing shortened smoke exists; no default `L = 200`/AC-040/AC-041/30-minute report exists. |

## Verification limits

- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity`:
  6 passed.
- Focused two-replica claim and tenant-fairness integration tests: 2 passed.
- Focused sustained 100-feature ingest journey: 1 passed.
- `git diff --check`: passed.
- Recorded formatter and lint evidence is green.
- The only recorded benchmark execution changed levels and durations, exited
  1, did not reach `L = 200`, and did not meet the AC-040 sample floor. No
  unmodified default benchmark result exists.

## Prior-finding closure

This is the first task-review attempt for this revision; there are no prior
`FIND-*` items to close. The follow-up rejected BHV-003, narrowed DUR-001 to
FIND-12, and independently preserved all retained IDs in
`findings-validation.md`.

## Verdict

**FIX_REQUIRED**

The candidate does not satisfy the approved task exactly. All thirteen retained
findings are bounded within the approved behavior and are packaged in
`TASK-008-CLOSEOUT-R1-capacity-closure.md` for `$wyrd-implement`.
