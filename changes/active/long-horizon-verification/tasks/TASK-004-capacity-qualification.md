---
id: TASK-004
kind: implementation
status: proposed
spec: SPEC-long-horizon-verification
spec_revision: 13
requirements: [REQ-015, AC-009, INV-001, INV-002, INV-003, INV-004, INV-005]
depends_on: [TASK-001, TASK-002, TASK-003]
---

# Qualify overhead and grouping capacity in the existing benchmark

## Outcome and Value

The single existing capacity binary reports and judges all five AC-009 cases,
preserves its old SLOs and proves assertion-only direct p99 <1 ms plus a grouped
noise-bounded overhead delta. It measures queue/discovery capacity and bounded
burst refusal/progress without a parallel benchmark or mandatory test lane.

## Owners, Scope, Consumers, and Prohibited Changes

Execute with `$wyrd-implement`. All INV-001–005 apply. Start from the integrated
TASK-001–003 candidate: groups, scheduled-contract consumers and bounded staging
must be real before final qualification. Do not mark those dependencies passed
from benchmark mocks. Own REQ-015/AC-009 and in-scope performance corrections
needed to meet the approved target, including a proven pre-existing baseline
miss; never relax either overhead threshold to pass.

Use crates/wyrd/wyrd-testing/src/bin/capacity/, mise run bench:capacity and
wyrd_verification_engine_overhead_seconds, retaining their metric definition.
This remains opt-in and outside every gate/verify lane. Preserve existing normal
mixed workload, required one/two-replica steps, command lifetime, artifact owner,
>=95% traffic, zero-error, p95 <10 ms, sample floor, <=1 s client queue drain
and <=60 s backlog drain rules. Intentional overloaded cases are separate and
do not excuse normal-load refusals. Keep profiling diagnostic. The benchmark's
existing host setup is not a new feature/runtime deployment prerequisite.
No synthetic host stress, distributed quota guarantee or new telemetry subsystem.

## Source-backed Reuse Map

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Capacity workflow | mise.toml bench:capacity; capacity main/load/step and release-server launcher | existing four-tenant mix and one/two-replica verdict steps; report.rs tests | Grouping/pressure cases absent | Extend same binary/driver, fixtures and report under existing command lifetime | No second binary, load generator, qualification lane or service launcher |
| Overhead evidence | ExecutionTelemetry::emit / overhead; metrics verification buckets; capacity Scrapes::overhead | evidence.rs::tests; report::overhead/step_row | Only direct p95 reported; no case isolation or noise comparison | Same metric, matched windows, sample/bucket evidence and p99/resolution | No wall-clock substitute or independent overhead timer |
| Pass/fail | report.rs::Report::{passed,write_to,render} and errors/backlog judgments | report::tests::every_slo_failure_fails_the_step and other existing report tests | Refusals all fail; memory report-only; no mandatory five-case verdict | Add explicit case verdicts and required evidence, isolate overload semantics, retain legacy verdict | No weakening old normal-load judgments to accommodate overload |
| Group/load measurements | Existing queue/backlog scrapes, Bifrost/read metrics, process resources and task 1/3 owners | capacity evidence.rs; existing report resource columns | No idle-expiry latency/read-cost or bounded staging evidence | Extend current samples/artifacts with source ceiling and per-tenant progress measurements | No persistent performance control plane or new group lifecycle machinery |

## Approach

1. Pin the integrated candidate/config and add failing metric/report tests for the two thresholds and mandatory cases.
2. Extend existing metric-derived evidence for isolated baseline/grouped p99 and disclosed repeatability noise.
3. Add grouped-submit, many-groups and controlled-overload cases to the same bounded capacity workflow.
4. Run the opt-in benchmark, diagnose target misses in existing owners, fix in-scope causes and rerun affected proof.
5. Retain final comparable Markdown/JSON evidence and the unchanged legacy verdict alongside all five case verdicts.

## Ordered Implementation Scenarios

### Scenario 1 — Measure both strict overhead bounds without confusing latency

**Behavior.** REQ-015/AC-009 cases 1/2: assertion-only direct no-group baseline
and grouped+owner case use matched warmed windows/config, >=1,000 samples each,
summed replica bucket deltas, direct-mode metric, p95 <10 ms and p99 <1 ms.
Upper bound exactly 1 ms, missing samples or insufficient resolution cannot pass.
Record repeated baseline variation/noise and grouped delta; a repeatable positive
delta beyond noise fails. Request/client phases supplement, not replace, engine
overhead. No new producer IO is hidden outside the engine interval.

**RED.** Add planned evidence::tests::direct_overhead_requires_resolved_p99 and
report::tests::grouped_overhead_preserves_both_targets in the existing capacity
binary (exact selectors below). Include absent data, strict threshold equality,
merged replica buckets and grouped/no-group isolation. Current Overhead exposes
only p95. Preserve existing normal-step report tests as regressions.

**GREEN.** Extend existing evidence/report owners with the required percentile,
resolution, sample/noise and case labels, not another metric. Record a baseline
miss as work still in scope; claim it pre-existing only with identified baseline
build evidence. Do not silently exclude slow cases or failed attempts.

**REFACTOR.** Share histogram/report logic with current owners while keeping old
normal-load SLO judgments unchanged. Rerun focused report/evidence regressions.

### Scenario 2 — Queue and many-group discovery have bounded, truthful evidence

**Behavior.** AC-009 cases 3/4: grouped submit rising load exposes throughput
knee/plateau and queue depth/age/drain. Many groups x N show idle-expiry-to-result
p50/p95/p99 and bounded Bifrost discovery/snapshot reads, query latency/rows/bytes.
Source ceiling+1 yields inconclusive/source_execution_limit_exceeded, never a
first-N judgment. All eligible known fixtures get correct serial group results
within the specified drain allowance after due; incomplete reads do not pass.

**RED.** Add planned report::tests::group_cases_require_complete_bounded_evidence.
Give it missing/delayed/incorrect group results, missing read costs, wrong ceiling
verdict and undrained queue cases. Current report has no such judged rows.

**GREEN.** Extend existing workload, query/backlog evidence and report for these
cases, with declared short Trigger durations and poll settings within the
existing command lifetime. Keep direct windows uncontaminated by other direct
workloads. Reuse the public Rust client and actual server grouping.

**REFACTOR.** Keep load generation and evidence in the current binary; remove
duplicate statement-count/coverage shortcuts. Preserve Scenario 1 and legacy steps.

### Scenario 3 — Above-capacity bursts remain bounded and do not starve B

**Behavior.** AC-009 case 5: include 300/s and 1,000/s ten-second offered points
alongside the measured knee. Count accepted/completed/refused traffic, with
correct typed retry timing, finite process staging, warmed memory plateau and
post-drain baseline variation. B's admitted work completes during A's continued
burst. Backlogs drain within 60 s of stopping; no unknown loss/error is excused.
Normal steps retain zero-error policy and report-only resources; overload
explicitly judges boundedness without distributed quota/fairness claims.

**RED.** Add planned report::tests::expected_overload_does_not_relax_normal_slos.
Feed correct expected pressure versus wrong errors, growing retained work/memory,
missing B progress and failed drains. Current report treats all refusals as
normal errors and resources as report-only.

**GREEN.** Add the explicit overload case and evidence, retaining original normal
SLO checks and separate legacy/extension verdicts. Do not fake flat memory
from one peak number or hide missed driver offers.

**REFACTOR.** Reuse current resource/backlog/time-series collection; keep failure
reasons inspectable and retain Scenario 1/2 and legacy regressions.

### Scenario 4 — Real opt-in qualification is complete and reproducible

**Behavior.** All AC-009 cases run on the integrated build and produce actual
artifacts under target/capacity with host/build/envelope/tenant/replica/rate/
window/Trigger/ceiling/noise metadata, raw metric evidence and correct verdicts.
The existing three verdict steps remain required. Missing or diagnostic-only
measurements cannot claim PASS; no gate/verify lane invokes the benchmark.

**RED.** Run the new focused report tests then the real mise run bench:capacity
qualification. Missing cases/targets must be visible as failures; no fabricated
latency or expected-error waiver can make an incomplete run pass.

**GREEN.** Complete all five cases and correct in-scope causes of measured misses.
Use matched repeated runs for delta/noise, and preserve both strict targets.
Keep performance corrections in their current runtime owners and record their
focused regression commands. If an expensive-to-reverse decision is required,
stop affected work through the skill rather than weaken the specification.

**REFACTOR.** Remove redundant reporting/measurement paths and rerun changed tests
and any affected benchmark case. Keep truthful raw artifacts and final source
revision tied to the result; no speculative performance framework.

## Acceptance Criteria

- Exact same overhead metric supports matched, sufficient-resolution p95/p99 evidence; both thresholds and grouped delta/noise criteria pass.
- All five cases meet AC-009's reports and pass/fail rules, including independent legacy and extension verdicts, empty/missing data failures and correct source-ceiling result.
- Existing production mix, traffic/error/sample/drain requirements and bounded benchmark lifetime remain; normal overload refusals are never reclassified as successful normal traffic.
- Burst staging/memory are bounded, typed refusal precedes staging, B progresses and every required backlog drains. Outbox crash loss remains an explicitly different durability scenario, not an excused loss during normal qualification.
- Benchmark remains opt-in/no gate or verify lane; profiling evidence is diagnostic and no new harness/binary exists.

## Expected Write Set and Consumer Closure

Likely owners: capacity main/load/step/evidence/report modules; its existing
public-client fixtures; server verification telemetry/app metrics only where
resolution/evidence requires; related existing process/outbox/queue/read
instrumentation; mise bench:capacity description/config only if needed for the
same command. Correct measured in-scope runtime overhead defects in the owner
that causes them, with regression proof; do not expand to a blanket optimization
or observability rewrite. Paths guide ownership, not private structure. Existing
SDK functional contracts/journeys stay owned by TASK-001–003; benchmark uses
one public Rust-client driver, not three new language performance harnesses.

## Verification and Evidence

The current manifest explicitly registers package wyrd-testing binary capacity;
report.rs/evidence.rs already have tests modules. The following new names are
planned and must be added there, then confirmed by nextest list. Run each exact
case during RED/GREEN:

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=evidence::tests::direct_overhead_requires_resolved_p99)'
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=report::tests::grouped_overhead_preserves_both_targets)'
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=report::tests::group_cases_require_complete_bounded_evidence)'
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=report::tests::expected_overload_does_not_relax_normal_slos)'
```

Existing report regressions, names confirmed in source:

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=report::tests::every_slo_failure_fails_the_step) | test(=report::tests::every_approved_error_category_fails_without_duplicate_judgment) | test(=report::tests::backlog_passes_only_within_sixty_seconds) | test(=report::tests::verdict_needs_every_verdict_step)'
```

Performance proof is opt-in and runs only through the existing environment/
launcher task, not by invoking a bare release binary or an unrelated stress tool:

```bash
mise run bench:capacity
mise run fmt
mise run lints
mise run check:deps
```

Inspect mise.toml's exact local-service/env/deadline prerequisites; the task owns
the Postgres/storage-emulator/release-replica lifecycle. Preserve its declared
host qualification; it is not a runtime portability prerequisite. Retain final
Markdown/JSON artifacts and repeatability comparisons under the same output
owner. If runtime/SQL/generated contracts change while fixing a proven target
miss, add only their affected focused regressions and applicable codegen:check/
check:tenant-isolation before completion; no full journey or gate aggregate.

Unit logic is TDD. Environment metadata, opt-in/no-gate membership and artifact
schema/source consistency use static/verification proof; do not manufacture RED
for already-correct wiring. Record every named added supporting test with its
exact command. No numbers are claimed before an actual run.

Append `$wyrd-implement`'s criterion/source/command/result table, with initial
misses, measured cause/fix, final target results, artifact paths, actual samples,
noise and limitations. An unmeasured or failed target is not IMPLEMENTED.

## Material Stop Conditions

Stop for a required change to a target/metric meaning, production mix, durability,
security, platform support or other approved material outcome; route through
`$wyrd-spec`, not a new benchmark or task. Local measurement mechanics and
source-backed performance corrections belong to `$wyrd-implement`. Missing
local setup is not approval to skip proof. Do not count pre-existing misses as
waived or claim performance from profiled/under-resolved data.

## Authority Links


- [Approved specification, revision 13](../spec.md).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../architecture/agent-rules.md).
- [Wyrd design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-doctrine.mdx), [Bifrost design](../../../../architecture/bifrost-design.md).
- [Spec workflow](../../../../architecture/references/languages/spec-driven-development.md), [implementation execution](../../../../architecture/references/languages/implementation-execution.md), [testing workflows](../../../../architecture/references/languages/testing-workflows.md).
- Execute with [$wyrd-implement](../../../../.agents/skills/wyrd-implement/SKILL.md); implementation returns evidence, not independent task approval.
