---
id: SPEC-l1-benchmark-protocol
revision: 2
status: approved
---

# Standardize L1 benchmarks

## Problem and agreed outcome

L1 cases currently share resources within an experiment, so running a case
alone can change the state or workload it measures. Detailed reports provide
evidence, but reviewers need a consistent category summary generated from that
evidence. Give every case its own lifecycle and record. Run cases through one
command per category and render one reproducible summary per sweep.

A **case** is one declared workload, fixture and load configuration. A
**sweep** is a category invocation over selected cases. A **phase** is part
of a case lifecycle. A **window** is one measured admission interval. The
[design](design/notes.md) explains the lifecycle and evidence boundaries. The
[case audit](design/case-audit.md#routine-cases-in-detail) names the routine cases
and identifies the targeted workloads.

## Required behavior

**REQ-001 — Category commands and selection.** Provide these opt-in mise tasks.

| Command | Default sweep |
|---|---|
| `mise run benchmark:l1:verification` | 5 Verification cases |
| `mise run benchmark:l1:gateway` | 5 Gateway cases |
| `mise run benchmark:l1:bifrost` | 18 Bifrost cases |

Use the audit's routine selections in declared order. Preserve the requested
order for explicit selections. Through the same task,
`-- --list-cases` lists exact selectors and routine or targeted status without
building or running a benchmark. `-- --cases <id>[,<id>...]` selects exactly
the requested cases. Reject unknown or ambiguous selectors before starting a
case. Qualify repeated Bifrost IDs by family where needed. The audit's other
workloads remain available for targeted execution, including intermediate Rust
client counts on the canonical workload. Support `-- --profile cpu|heap|both`
with or without case selection. Migrate current L1 task callers and benchmark
guidance to the category commands and terminology.

**REQ-002 — Fresh state and cleanup.** A case uses the same declared workload
and preparation when selected alone or in any sweep. Give each case fresh
server and PostgreSQL processes and data, WAL and object storage, clients,
and any SDK or provider processes it uses. Verify fixture and resource
ownership before warmup and retain isolation and teardown evidence. Keep
the case's own state through warmup and measurement. Cases run sequentially.
A failed case can be followed by another once cleanup is confirmed. If cleanup
cannot be confirmed within the teardown allowance, record the invalid attempt
and stop the sweep, marking the remaining selected cases unstarted.

**REQ-003 — Timing and measurement boundaries.** Prepare build artifacts
before the case clock. Use the accepted per-case phase allowances below.
The persist and report phase covers the individual case result. The category
table renders once from the saved sweep after case execution ends.

| Phase | Maximum |
|---|---:|
| Setup | 240 s |
| Warmup | 30 s |
| Measurement | 90 s, three 30-second windows |
| Drain and capture, including between windows | 150 s total |
| Teardown | 60 s |
| Persist and report | 20 s |
| Total | 590 s |

Freeze completed work and interval CPU and counters at each window stop.
Account separately for late completions and work done during drain. Drain
in-flight requests before opening the next window. Keep the existing case
correctness, custody and sample-floor checks. Capture outstanding publication
and maintenance work before teardown. Write success is bounded by durable
acknowledgement as defined by [Bifrost durability and visibility](../../../architecture/bifrost-design.md#durability-and-visibility).
A missed deadline, failed integrity check, or missing required lifecycle or
measurement evidence makes the attempt invalid. Record overruns and observed
cleanup failures, including failures of external services to stop in time.

**REQ-004 — Aggregates and measurement validity.** Calculate rates and efficiency
from sums of their matching raw numerators and denominators. Pool histogram counts for
case-level percentiles, preserving sample counts and bucket bounds. All previous
performance targets are void for the refactored L1 cases; do not inherit their
thresholds or operators. This change establishes no replacement performance targets.
Keep per-window correctness checks, sample floors and detailed readings.
No performance-target judgment is required for these cases.
Distinguish unavailable and inapplicable readings from zero.

**REQ-005 — Case and sweep records.** Persist a full JSON record for each
attempted case. Keep identity and provenance, detailed window and aggregate
evidence, phase timings, fixture and correctness evidence, drain and cleanup
outcomes, verdict, and profile references or capture notes when applicable.
Keep a sweep record with the invocation, mode and ordered selection, each
case's outcome, and record references or reasons for unstarted cases. If
persistence exceeds its allowance or fails, the sweep reports the case as
invalid and includes the persistence reason. Mark its record reference
unavailable if the case record could not be saved.
Preserve the inputs needed for reports to regenerate without live services
or temporary artifacts. Version the new protocol and preserve original interpretation of
historical records. Prevent new records from silently entering existing
comparison or calibration paths that assume the old protocol.

**REQ-006 — Deterministic review output.** End the sweep with one Markdown
table. The existing `capacity report` entry reproduces the same table from
the same saved sweep with the same reader version and renders the full report
from a case record. Print coverage, mode and sweep path. Use one row per
selected case in the saved sweep order, with case, status, throughput, server CPU
per unit, labeled headline latency, target and case record reference. Bifrost
adds an input column for OTLP input MiB/s and input MiB/server CPU-second.
Use the workload-specific readings in the [summary design](design/notes.md#5-category-sweeps-and-deterministic-summaries).
Do not apply previous targets in ordinary or profiled runs; target output must
not imply that a legacy threshold still applies. Mark profiled readings diagnostic.
Give invalid and unstarted rows their reasons
and omit their performance cells. Benchmark contributor guidance directs
reviewers to paste this table verbatim and add interpretation in prose.

**REQ-007 — Smoke workflow.** Put short L1 smoke checks in the harness test
lane. Exercise the normal sweep path with small fixtures and short windows,
checking workflow and evidence integrity. Include at least two sequential
cases so the check can detect state carryover. The performance commands use
the timing in REQ-003.

## Scope and delivery

Deliver as one implementation task after spec approval. Reuse the benchmark
runner, recorder, profiling and report entry points. The refactor changes L1
execution and evidence. Production workload and durability semantics remain
governed by their existing owners. Baseline selection, comparison methodology,
performance-change qualification and L2 calibration policy are deferred.

## Acceptance

| ID | Observation |
|---|---|
| AC-001 | Each category resolves its exact routine selection. Listed targeted cases are selectable and bad selectors fail before launch. Selecting a case alone, in a subset or in a reordered sweep preserves its declaration. Profiled execution retains capture evidence and diagnostic labeling. (REQ-001, REQ-002, REQ-006) |
| AC-002 | A short smoke sweep exercises startup, execution, drain, cleanup, persistence and rendering for multiple cases. Evidence confirms fresh resources at the case boundary. An invalid case with proven cleanup permits the next case to run. Unconfirmed cleanup stops later launches and keeps their rows visible. (REQ-002, REQ-003, REQ-005, REQ-007) |
| AC-003 | Controlled measurement checks distinguish late work from interval totals and demonstrate aggregate rates and pooled latency with unequal window work. Correctness and evidence-integrity failures invalidate attempts; previous performance targets do not apply. (REQ-003, REQ-004) |
| AC-004 | A saved sweep reproduces its emitted Markdown after temporary artifacts are removed. Its case records still reproduce detailed reports. Partial, invalid and profiled results retain their statuses, and legacy readers keep the old protocol interpretation. (REQ-005, REQ-006) |
| AC-005 | When full-size performance execution resumes, verify per-phase timing and end-to-end feasibility on supported benchmark hosts. Report overruns as invalid attempts. (REQ-003) |

Revision 1 was approved by the maintainer on 2026-11-22. Full-size
feasibility remains unverified while performance execution is on hold.

Revision 2 records the maintainer's explicit decision in the task-review
follow-up: all previous performance targets are void because these are
refactored benchmarks/cases. The maintainer approved revising the packet on
that basis. Correctness, custody, sample-floor and lifecycle checks remain
required; no replacement performance targets are introduced. Historical
records retain their original interpretation under REQ-005.
