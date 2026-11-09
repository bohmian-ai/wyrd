---
id: TASK-002
kind: research
status: proposed
spec: SPEC-benchmark-latency-diagnosis
spec_revision: 2
requirements: [REQ-005, REQ-006, REQ-007, REQ-008, INV-001, INV-002, INV-003, INV-004, AC-003, AC-004]
depends_on: [TASK-001]
---

# Explain c64 Gateway capture latency and propose the next work

## Outcome and Value

Deliver a diagnosis the maintainer can use to choose the next latency change.
Explain where client and Gateway time goes in the existing c64 capture cases,
what the evidence establishes, and which material uncertainties remain. Produce
ranked corrective proposals or the smallest measurement needed to select one.

The objectives are client completion p50 below 1 ms and Gateway-added p50 below
500 us. Evaluate these independently; meeting the server-overhead objective does
not establish the client objective. Do not replace the existing p99 benchmark
judgments or claim that this research achieves a performance improvement.

Write `research/gateway-latency.md` in this packet, with supporting analysis only
where needed for reproducibility. Final attribution uses TASK-001's completed
measurement contract and consumer inventory. Existing-artifact inventory and
source research may start earlier; the final handoff depends on that contract.

## What Changes and What Must Stay True

This task changes research documents only. It neither implements an optimization
nor adds production instrumentation. It does not run a benchmark, rebuild a
baseline, start a release build, install profiling dependencies or generate
synthetic host load. Ordinary read-only analysis of existing artifacts, including
viewing an existing profile, is in scope.

Separate measured facts, source-proven mechanisms, hypotheses and unknowns.
Preserve history and raw profiles. Capture-slot backpressure requires evidence
about that outbox; Scribe memory ownership or a slow Scribe acknowledgement is not
proof of request waiting. Work performed after sending an answer may still share
workers with its waiting handler; prove the mechanism separately from its
contribution to measured latency. CPU sample proportions are not request wall-time
proportions and cannot establish off-CPU waits.

Maintain the approved security, tenancy, capture, admission, response, settlement
and durability constraints in every proposal. No new scheduler, cache, allocator,
transport or memory-accounting framework is selected. Every suggested correction
names the existing owner, the cause it would break, its limits and its proof.

Local profiles may be incomplete or no longer symbolizable with the current
binary. The profiling binary was rebuilt during the interrupted work. Check its
identity against the saved profile; do not substitute the new binary or trust a
derived summary without verifying its symbols and selected interval. Missing
symbols are an evidence gap, not a reason to rerun the baseline.

## Approach

1. Inventory existing records, raw timings, profiles and their source/build
   identities; classify missing, invalid or incomparable evidence explicitly.
2. Reconstruct the request-to-client and post-answer workflows from their current
   owners and the committed source corresponding to each historical record.
3. Recompute the relevant recorded distributions and compare c64 cases with c1
   and streaming controls, keeping profiling modes and repetitions separate.
4. Attribute supported CPU/allocation costs and waiting mechanisms; identify the
   blind spots against the finalized benchmark observation contract.
5. Write a ranked diagnosis and concrete correction/measurement proposals, each
   with supporting evidence, an expected distinguishing result and stopping rule.

## Proof Strategy

TDD is not applicable: this task introduces no executable behavior. Its proof is
an independently checkable report with traceable artifact references, calculations,
source paths/symbols and explicit limits. Do not manufacture a failing latency test
or add a reusable analysis program under a documentation-only task.

For each of the existing 1 KiB metadata, 64 KiB metadata and 64 KiB payload c64
cases, report client completion, server elapsed, per-request residual overhead,
provider exchange time, throughput and CPU/work from their declared populations.
Report available quantiles and repetitions without inventing missing readings.
Use c1 to distinguish per-call work from concurrency effects and streaming to
check handoff/lifetime differences. Do not subtract medians or sum stage quantiles
to construct a latency budget.

Follow the critical path across authentication/body validation, snapshot and
routing, admission and capture reservation, execution dispatch, provider exchange,
accounting, answer send/resumption and response serialization/transfer/validation.
Map capture projection, outbox scheduling, encoding, WAL/durable submission and
drain separately. Mark which segments have measurements and which only have
source evidence or an unmeasured hypothesis.

A useful uncertainty is a falsifiable statement. For example, an admission-wait
hypothesis must name a capture-reservation observation and a workload/observation
that would contradict it. A worker-contention hypothesis must distinguish
post-answer CPU work from answer-delivery waiting; a sampled stack alone is
insufficient. Do not preselect either explanation.

## Acceptance Criteria

| Spec obligations | Proof and expected result |
|---|---|
| REQ-005; AC-003 | A source-backed critical-path map distinguishes response work, shared-worker post-answer work, outbox work and durable submission. A claimed measured contribution without the corresponding boundary/population fails. |
| REQ-006; AC-003 | The c64 comparison assesses both p50 objectives and explains client validation and provider-exchange exclusions. Client/server clock boundaries remain separate; a residual is not represented as every possible server-induced delay. |
| REQ-007; INV-004; AC-003 | Each numeric/profile claim traces to source/build identity, cases, repetitions and profiling mode. Missing symbols/window alignment is disclosed. No baseline rerun, changed history or unsupported controlled-improvement claim occurs. |
| REQ-008; INV-001–003; AC-004 | Ranked proposals name supported causes, owner, expected effect, risks, regression proof and rejection evidence. Unknowns have bounded measurement proposals, not invented root causes. No optimization is implemented. |

## Implementation Reference

Inspected workflows and remaining diagnostic gaps:

- `gateway/invocation.rs::GatewayInvocation::{govern,admit_call,capture_slot,capture}`
  and `CallTask::{run,settle_buffered,settle_stream}` define reservation, task
  ownership, answer send and capture order. `Answer` and
  `telemetry.rs::CallTelemetry` include unfinished send/resume and capture-slot
  observations. They are candidate observations, not historical evidence or a
  selected concurrency change.
- `gateway/capture.rs::{CallCapture,redacted_payload}` and
  `scribe_outbox.rs::{ScribeSink,ScribeWrite}` own projection, redaction, batching
  and delivery. `wyrd-runtime::outbox::{Outbox,Reservation}` owns reserved capacity,
  admission waiting, retry and release. Trace these owners together; a slow
  downstream stage is not automatically on the current request's path.
- `wyrd-gateway::GatewayEngine` and adapter HTTP dispatch use
  `skald-providers::exchange::{scope,ExchangeTimer}`. Exchange time includes local
  transport and resumption effects, not solely remote provider computation.
- `src/bin/capacity/gateway/workload.rs::{call,once,buffered,Timed}` timestamps
  client completion after full receipt and validation. `gateway/run.rs::{drive,
  Samples,Observed}` drives closed-loop c1/c64 cases and records their populations.
  `gateway/capacity.rs` owns open-loop phase/ownership accounting. Reuse these
  workload definitions; do not invent a replacement driver.
- `capacity::Population`, `experiment::Record`, `profile::{Profiler,HeapProfiler}`
  and the saved per-window timing snapshots own distributions and diagnostic
  evidence. TASK-001 closes their single-recorder consumer path; its existence
  supplies no new historical stage observations.

Existing artifact starting set, relative to the repository root:

| Evidence | Record/artifact |
|---|---|
| Gateway baseline, unprofiled | `benchmarks/history/gateway/overhead/20261108T234942Z-5508cf29f.json` |
| Gateway baseline profile | `target/benchmarks/gateway/overhead/20261109T002859Z-5508cf29f.json` and its artifact directory |
| Gateway candidate, unprofiled | `benchmarks/history/gateway/overhead/20261109T153318Z-b1cf95f2e.json` |
| Gateway candidate profile | `target/benchmarks/gateway/overhead/20261109T153854Z-b1cf95f2e.json`, raw `20261109T153854Z/profiles/profile-2483944.json.gz`, per-step heap captures and `timings/` |
| Existing selected-step candidate rerun | `target/benchmarks/gateway/overhead/20261109T154508Z-b1cf95f2e.json` |

Verify availability and validity before use. Verification records may corroborate
shared-owner behavior, but do not substitute for Gateway populations or expand
this task into a Verification performance investigation.

## Verification and Evidence

No tests, builds or benchmark commands execute for this documentation-only task.
The report must allow a second reader to reproduce each calculation from the
identified raw record/distribution. Check local links and source symbols, distinguish
committed historical source from the unfinished worktree, and inspect the document
diff. Do not claim runtime verification from source inspection.

If existing evidence is insufficient, include a bounded proposed experiment—not
an executed command sequence—with:

- the hypothesis, missing observation and result that would falsify it;
- exact existing driver/task, selected cases/concurrency, offered-load model,
  profiling mode, repetitions and stopping condition;
- expected counts, validity checks, required retained artifacts and interpretation;
- instrumentation/observer effects and why existing evidence cannot answer it.

The current `mise.toml` owners are `bench:l1:gateway`, its declared `--steps` and
`--pace` diagnostics, and the existing optional profile workflow in
`benchmarks/README.md`. Record an exact runnable command only after selecting the
minimum concrete experiment; present it for separate maintainer authorization.
Neither this plan nor an implementation request for this research task authorizes
that experiment. Never rerun the baseline.

## Material Stop Conditions

Stop for planning/specification authority if the deliverable requires executable
instrumentation, an optimization, changed benchmark judgments or weakened capture,
security, ownership or durability semantics. Report an unavailable artifact or
missing symbols as an explicit evidence gap; do not manufacture a root cause,
rebuild a historical baseline or expand into fresh benchmarking to close it.
Routine read-only source analysis and document organization remain task-owned.

## Authority Links

- [Approved spec revision 2](../spec.md)
- [Recorder prerequisite](TASK-001-sole-benchmark-recorder.md)
- [Workflow](../../../spec-driven-development.md),
  [implementation execution](../../../implementation-execution.md),
  [testing](../../../../TESTING.md)
- [Capture contract](../../../../architecture/wyrd-design.md#principal-model),
  [Bifrost](../../../../architecture/bifrost-design.md),
  [operational indicators](../../../../architecture/operations/reliability-and-recovery.md)
- [Benchmark definitions](../../../../benchmarks/README.md)
