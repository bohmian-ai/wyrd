# Focused follow-up review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd`
- Candidate: `852894689388124960993014a46934e73c0ed2a8`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 57, specifically REQ-171, AC-040, AC-041, and the revision 57 history entry
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`, section “bench:capacity implementation (revision 57)”

## Paths and source inspected

- `changes/active/verified-change-contract/spec.md:1706-1758,2347-2370,2436-2455`
- `changes/active/verified-change-contract/tasks/task-008-closeout.md:1485-1569`
- `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:188-246`
- `crates/wyrd/wyrd-testing/src/bin/capacity/load.rs:119-143,230-307,345-425,449-483`
- `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:264-330`
- `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:143-175,210-275`
- `crates/wyrd/wyrd-testing/src/bin/capacity/report.rs:1-19,129-210,240-348,417-479`
- Discovery claims `DUR-001`, `BHV-003`, and `CAP-002` in this review directory

## Conflict A — error accounting versus test-owned correctness and durability

**Resolution: DUR-001 is partly rejected and partly revised.**

The operative REQ-171 Errors SLO is explicit: a request that failed, was refused, was lost, or produced a wrong judgment is an error, and the required value is zero (`spec.md:1736-1745`). That exact normative text controls the more general revision-history statement that judgment correctness and exactly-once claims moved to tests (`spec.md:2450-2451`). The two statements are compatible when “move to tests” is read as ownership of the comprehensive correctness and durability claims, not permission for a capacity report to ignore an observed error category that REQ-171 expressly names.

Accordingly:

- Direct-response verdict comparison and `wrong_verdicts` accounting are **required**, not prohibited. `Request::send` knows the expected result of the input it selected (`load.rs:345-365`), and `Lane::drive` records a mismatch (`load.rs:291-295`). Promoting that observed mismatch to the zero-error cell (`step.rs:288-293`) implements the literal “produced a wrong judgment” SLI. It does not replace the deterministic Rust SDK judgment tests recorded by the task.
- An accepted queued request for which no durable run exists after the run backlog drains is a **lost request**, so the `lost` accounting at `step.rs:306-309` is also required by the literal Errors SLI. The `drained` guard prevents an outstanding run from being mislabeled as loss. Comprehensive queue durability remains test-owned.
- Stable request refusals and terminal failed-run statuses are likewise within the required Errors SLI (`step.rs:285-287,314-320`).
- A `duplicate run` is different. REQ-171's Errors row does not name duplication, while the same approved section assigns “exactly-once queued claims across replicas” to tests and says nothing else is judged (`spec.md:1751-1756`). Making `runs.created > tally.accepted` a benchmark-failing error (`step.rs:310-312`) reintroduces one half of the exactly-once claim that revision 57 deliberately placed in the cross-replica integration test. The report advertises this extra verdict input at `report.rs:7-8,447`. That part of DUR-001 is confirmed as bounded scope drift; removing it does not permit loss or wrong judgments to pass.

### Revised proposed finding FU-001 — DRIFT: duplicate-run detection is still a benchmark verdict

- **Discovery source:** revised from `DUR-001`.
- **Violated obligation:** REQ-171 limits benchmark judgment to its stated SLOs, says nothing else is judged, and assigns exactly-once queued claims across replicas to tests (`spec.md:1736-1756`, revision history `2450-2451`).
- **Exact location:** `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:310-312`; `crates/wyrd/wyrd-testing/src/bin/capacity/report.rs:7-8,447`.
- **Evidence:** `Deployment::ops` converts `runs.created > tally.accepted` into `duplicate run`, and `report::errors` makes every such entry fail the step. The rendered benchmark contract explicitly lists duplication as a judged error even though duplicate/exactly-once proof was moved to `pg_verification_runtime.rs::two_replicas_claim_each_queued_run_exactly_once`.
- **Observable consequence:** the capacity verdict can fail on an exactly-once assertion outside its approved SLO set, duplicating the test-owned durability contract and weakening the intended diagnostic separation.
- **Testable correction:** stop producing and advertising `duplicate run` as a benchmark error while preserving durable-run reads used for queued traffic completion and backlog, and preserve the explicit REQ-171 accounting for refusals, request failures, lost requests, and wrong direct judgments. A focused report/step test must show that only the approved error categories affect the cell; the existing cross-replica test remains the exactly-once proof.

## Conflict B — `K = 50` and the AC-040 sample floor

**Resolution: BHV-003 is rejected as an implementation finding.**

The arithmetic is correct: at `K = 50`, direct verification is `L/2 = 25/s`; five equal kinds receive `5/s` each across all four tenants; over 180 seconds the maximum offered count is 900 per kind (`spec.md:1714-1735`; `load.rs:449-483`; `main.rs:219-226`). `report::overhead` correctly refuses to pass the one-replica sustained step below 1,000 samples (`report.rs:184-210`).

Neither REQ-171 nor AC-040 promises that every possible measured knee produces a passing final verdict. They define the measurement and the pass conditions: the sustained step is run at the measured knee, the run passes only if the verdict steps pass, and AC-040 requires at least 1,000 samples in that one-replica sustained step. Therefore a deployment whose knee is only 50 legitimately returns **FAIL** because it cannot supply the required AC-040 evidence. Treating that honest failure as an implementation defect would require the harness to manufacture samples outside the approved production mix or change a locked duration, rate, or knee.

There is no correction within approved revision 57 behavior that structurally guarantees 1,000 samples at `K = 50`: oversampling a kind changes the equal-share workload; extending the step changes the fixed sequence and duration; raising the lowest allowed knee changes the ramp contract; and relaxing the floor changes AC-040. A future product requirement that a `K = 50` deployment be capable of passing despite this arithmetic would require a specification revision. The current implementation instead does the required safe thing by failing the cell and run. This resolution does not close the separate evidence question of whether an authoritative default run was actually recorded.

## Report-shape uncertainty — CAP-002

**Resolution: CAP-002 is confirmed.**

The exact REQ-171 sentence does not permit two differently headed tables. It specifies “One table,” step rows followed by per-operation rows, and “the same columns” (`spec.md:1751-1753`). `Report::render` emits a completed `## Steps` table with `backlogs drained`, `CPU / memory`, and `result` columns (`report.rs:449-462`), then opens a separate `## Per operation` table with `backlog left` and `driver` and no result column (`report.rs:463-477`). Although both row builders internally use `Row`, the rendered comparison surface has two schemas and two headers; that is the user-visible contract the requirement governs.

The smallest correction is the one already described by CAP-002: render step and operation rows consecutively under one shared schema and prove by a rendering test that there is one header and consistent column count/meaning. No new finding is added for this conflict.

## Result

**RESOLVED**

- `DUR-001`: revised to `FU-001`; wrong-judgment and lost-request accounting are required, while duplicate-run judgment is out-of-scope exactly-once drift.
- `BHV-003`: rejected as an implementation finding; `K = 50` is allowed to yield a legitimate failing benchmark result, and changing that outcome would require approved spec revision.
- `CAP-002`: confirmed; separate differently headed tables do not satisfy the singular same-column report contract.
