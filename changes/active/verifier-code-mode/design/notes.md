# Verification benchmarks across language SDKs

Status: finalized

## Current decision — 2026-11-12 alignment correction

The maintainer agreed that the same verification benchmark must run through
Rust, Python and TypeScript, with actionable phase latency, CPU and memory
breakdowns on both client and server. Missing Python/TS shared-core timings or
native/managed allocation coverage cannot satisfy closeout. Compile benchmark
measurement machinery out of production artifacts behind bench; merely leaving
it inactive or allocation-free is insufficient. Preserve normal operational
metrics/OTEL and the shared public SDK workflow.

Revision 7 supersedes revision 6's blanket ban on SDK bench feature wiring:
minimum forwarding of the existing bench capability and local harness wiring
is permitted for diagnostics. No duplicate recorder, public SDK benchmark API,
separate SDK product or production benchmark activation is introduced. Remove
unnecessary S4 probes/collectors; retain useful minimal collection where it
actually serves the required breakdown. Bench-enabled release artifacts are
identified local benchmark outputs, excluded from production wheel/npm builds.

Spec revision 7 and TASK-003-R1 own this scope and closeout. Earlier broader
profiling and later external-only/gap-permissive proposals are historical and
superseded. Planning does not mark TASK-003 passed.

## Problem

The maintainer wants publishable end-to-end verification measurements and client
profiling for Python and TypeScript as well as Rust, because most expected
users will use those SDKs. The current verification benchmark drives the Rust
SDK against a Rust server. Native bindings share that core, but Rust-only
measurements omit language input/output conversion and scheduling overhead.
Resource-scaling advanced beyond TASK-002B's previous source; the refreshed
planning pin now records the committed version being composed.

## Desired outcome

Run an equivalent public verification journey in each first-class language,
show comparable client and server measurements, and identify client bottlenecks
from profiles and reached-phase measurements. Keep one authoritative fixture,
server, qualification policy, record format and reporting workflow.

On 2026-11-11 the maintainer agreed to the presented recommendation and
requested `$wyrd-plan`: native three-language callers, Python threaded-sync
concurrency, separate call-return/result-readiness observations, measured
Python/TS baselines and retained Rust objectives. Revision 4 and proposed
TASK-002B/TASK-003 record this scope; private profiling tool/runner mechanics
remain implementation-owned and must prove the required coverage.

## Constraints and non-goals

- `server` means server-induced latency; `client` means end-to-end
  client–server–client latency. Include all elapsed work; phases/profiles
  explain it without subtraction.
- Same release Rust server build, server limits, workload/evidence, local mock
  behavior and result contract across language runs. Compare independent runs,
  not all languages competing against one server simultaneously.
- Keep native Python/TypeScript bindings and the shared wyrd-client transport
  in the measured path; raw HTTP would not establish SDK performance.
- Unprofiled qualification and profiled diagnosis remain distinct. Ordinary
  SDK calls must not install benchmark recorders/profilers or collect data.
- No three parallel benchmark frameworks, new server/public timing API,
  benchmark auth bypass, general telemetry framework or new SDK async facade.
- Do not infer client cost by subtracting language p99 from Rust p99, or server
  p99 from client p99. Profile each path and pair observations within requests
  when subtraction is used. CPU samples alone do not measure blocking waits.
- Current staged merge, prior task evidence and local work remain untouched.

## Proposed implementation

### One experiment family, three native drivers

Extend the existing capacity orchestrator's workload/driver selection to run
Rust, Python and TypeScript as separately identified benchmark populations.
Use small language-native entrypoints for the public SDK calls. Reuse the same
provisioning, fixed Cards/programs, mock conversations, stop/drain lifecycle,
server snapshots, reconciliation and standard record/report renderer. One
experiment can produce independently labelled records/rows per language; do not
average them into one result. Exact CLI shape is private implementation work.

First cover immediate `observe.verify` at 1 and 64 in-flight calls using
TASK-003's same canonical one-Assert/two-Agent workload. Reuse the current
Assert-only baseline as a cheap diagnostic control: its small server cost makes
binding/transport/conversion overhead easier to see. Do not extend L2,
submit/wait, cold startup or arbitrary workload combinations in this first
addition. Typed custom-data decoding is a separate labelled case if selected;
do not quietly compare Python model validation with untyped Rust/TS results.

Run one application client process per language with the same recorded driver
CPU budget. Candidate representative modes: Rust's existing bounded async
calls; Node's bounded Promises; Python's existing synchronous public API over a
fixed prestarted thread pool. Python detaches the GIL while the shared IO runs;
conversion remains real Python work. At c64, prestart workers outside measured
windows and keep 64 independent closed-loop workers, each timing its own call.
Record the mode and worker count; do not label this native async Python or
compare 64 Python processes against one Node/Rust process without disclosure.
The existing orchestrator and mock process are separate from the language
client; account for each role rather than charging fixture CPU to the SDK.

### End-to-end and result-readiness boundaries

Prepare connections, auth warmup, state/run handles, fixed input data and worker
startup outside the steady-state timer. Start the language-local monotonic
clock immediately before the public verify invocation and stop when the normal
call returns/resolves. Include input conversion, native boundary, shared SDK
preparation/serialization, transport, server work, decode and language return
conversion actually reached by that API.

Python currently materializes requirement dictionaries on property access,
whereas TS exposes parsed objects. To avoid hiding deferred conversion, also
measure a clearly labelled result-ready boundary that includes materializing
the agreed compact verdict, requirements, citations and custom-data view in
that language. Record call-return and result-materialization phases separately;
choose the same required fields rather than forcing every language to serialize
its whole object again. Correctness assertions and benchmark bookkeeping stay
outside the timed operations. The maintainer agreed to retain TASK-003's call-return metric and add the
result-ready sample separately. Preserve the historical call-return meaning
and do not silently rename it.

Server-side checks/tool records are fetched through the latest explicit details
API outside the immediate latency sample, with ordinary publication/read rules.
They prove both Agents/code operations occurred without smuggling a second
request into the verify timer. Failed verdicts are successfully completed calls;
transport/error/cancellation populations must remain visible and reconciled.

### Measurements and profiles

For each language and client count, publish client p50/p95/p99, throughput,
errors/late completions, corresponding server distributions, client and server
CPU/RSS, and existing phase/capture populations. Persist runtime versions,
SDK/package/native build identity, bindings, process roles, concurrency mode,
fixture version and whether profiles were enabled. Schema-3 generic controls
and diagnostic readings are the first reuse point.

Reuse the existing shared recorder and phase owners across all SDK bench
processes. Record language input/return/materialization around shared native
preparation/authentication/HTTP/body/decode observations. Server observations
cover admission/input, orchestration/workspace, Gateway/provider, sandbox /
IPC/host waits and result assembly. Preserve inclusive envelopes and full client
time; map CPU/allocation findings to phases and their process/window without
inventing exclusive sums across overlapping tasks or GC work.

Use established runtime/process CPU and managed-memory tools alongside the
existing native heap owner. Actual Python/V8 and native SDK allocation sites,
allocation volume and retained memory must be demonstrated, not inferred from
RSS, synthetic probes, a native-call placeholder or server profiles. Missing
required domains blocks diagnostic closeout. Ordinary production server and SDK
builds must compile out benchmark machinery; use isolated feature/package proof
and actual wheel/npm archives, not only runtime-disabled tests. No duplicate
collection system or new public SDK benchmark interface is proposed.

Establish total-client baselines for all three SDKs. The 2026-11-12 direction
retires adjusted-client subtraction and its target; retain the separate
historical server-overhead criterion.
Profiling identifies where time went; it does not imply the profiled timing is
qualification evidence. More runs and foreign-runtime profiling complexity are
the principal costs of the three-language matrix.

### Task and authority handoff

1. Refresh TASK-002B's source pin/range and integration proof against latest
   committed resource-scaling, preserving TASK-002A's existing reconciliation.
   Do not make TASK-002B implement three-language drivers or optimization work.
2. Reconcile approved verifier behavior with the incoming compact-result /
   explicit-details API and source benchmark/runtime changes. Capture the
   language scope and timer/result-readiness/qualification decisions in a
   human-approved verifier specification revision before revising acceptance.
3. Extend TASK-003's existing benchmark owners with the shared workload and
   native language runners, record/report closure and focused proof; TASK-003-R1 now removes the custom profile subsystem and fixes disabled production costs.
   Split out a task only if the concrete write set has a real independent
   outcome; do not create a framework or task per language by default.

## Evidence and rationale

Inspected on 2026-11-11:

- Resource-scaling HEAD: `4b37d47fce2ec01d26925f184a19892ae4cdb2ba` at planning; the earlier
  design inspection used `92b7278de`.
  Its `benchmark-latency-diagnosis/spec.md` is approved revision 7; TASK-005
  covers client/server collection and compact/detail migration, TASK-006 adds
  local approximate Gateway governance and Agent latency work. This is materially
  beyond TASK-002B's previous `c9a2e27e6` revision-4 source; it is not a path-only
  refresh. The current pin commits TASK-005/006 R1 remediation evidence and original
  FIX_REQUIRED review reports; it does not establish independent closure.
  It is also divergent/rebased from old `8d25b13a1`/`c9a2e27e6` inputs, sharing
  ancestor `5234961d5`; patch correspondence and material differences must be
  checked. Imports distinguish committed evidence from untracked source work.
- Destination HEAD remains `bcf61769e`; MERGE_HEAD remains `8d25b13a1`.
  No pending integration/index or existing task was changed by this design.
- `capacity/verification/run.rs::{Bench,drive,call}` in resource-scaling uses
  `WyrdState` in Rust. The source now also has a model `Judge` operation,
  `JUDGE_VERIFIER` and a local mocked Verdict. That operation alone is not proof
  of TASK-003's specified two-Agent/two-code workload or complete client attribution.
- `wyrd-testing/src/profile.rs::{Capture,HeapProfiler,ProfileMode}` now records
  server and driver CPU/heap profiles. `wyrd-benchmark::heap` requires the
  owning binary's allocator hooks and installs no allocator itself. This is
  reusable orchestration/native instrumentation, not verified Python/V8 coverage.
- Python `src/observe/mod.rs::verify` converts inputs, calls shared
  `observe.verify` through `trace::block_on`, then wraps `PyVerificationResult`.
  `trace::block_on` detaches the GIL while IO runs. `src/verify/mod.rs::plain`
  wraps Rust data and `requirements` converts it to Python on access. Existing
  public Python tests use that synchronous API; no verify_async is exposed.
- TypeScript `wyrd/src/index.ts::Observe::verify` converts inputs, awaits native
  verify and returns the projected result; native `cards.rs::verify` uses the
  same shared Rust observe path. These language conversion/scheduling layers
  are absent from a Rust-only benchmark, even though transport is shared.
- Source `wyrd-spec/src/verification/execution.rs` and owning Verifier architecture
  now separate compact `VerificationResult` from `VerificationDetails`, with
  explicit HTTP/MCP/SDK details reads. Old verifier REQ-015–017/task assertions
  require record retention/readback and must be mapped explicitly to the new
  surfaces; neither silently dropping those proofs nor restoring large default
  results is an acceptable integration shortcut.
- Shared memory search returned maintainer style preferences, but no existing
  approved cross-language benchmark design. Source CodeGraph was used before
  further code tracing; no profiling/load experiment was run.

## Decisions and alternatives

User outcome: Python and TypeScript client performance/profiling matter and
should be publishable alongside Rust; server remains Rust.

Recommendation: one shared experiment with distinct language drivers and
populations. Three independent benchmark implementations would duplicate fixture,
validity and reporting rules. A raw-HTTP or Rust-only driver cannot establish
language SDK costs. Do not infer an average user latency by combining languages.

The previous payload-sizing discussion ended with leaving Gateway payload sizes
unchanged; this proposal does not reopen that decision.

## Standards conflicts

Verifier revision 3 selected the Rust SDK and older immediate result-record
projection. Agreed revision 4 now records language qualification and maps
complete records to the source's approved explicit details owner, with focused
public projection proof. Prior TASK-001/002 acceptance remains historical under
revision 2, not silently invalidated or relabelled as new-contract acceptance.
The source's revised Gateway quota contract also needs scoped integration
review under its authority; language benchmarking does not approve governance
changes or infer their performance impact from an unconfigured-quota fixture.

## Open questions

The maintainer approved the three required outcomes and compile-time bench
boundary. Tool choice and minimum private wiring must be demonstrated on actual
SDK calls; no external profiler capability is assumed. If a required phase or
native/managed allocation domain cannot be obtained within the shared bench
owners, report the concrete blocker before closeout. Do not weaken the
requirement or ship the missing machinery in normal packages. Clean full
qualification and independent task review remain required.

## Handoff

The maintainer requested cleanup planning after source review. Spec revision 7
and TASK-003-R1 record the revised authority and remaining work; TASK-003 now
points to that execution entry point and retains its historical evidence.
No runtime implementation, profiler experiment, benchmark, Git commit or task
PASS is claimed by this design/planning edit.

## Total-time clarification — 2026-11-12

The maintainer requires ALL elapsed client time and sufficient metrics or
profiling to identify latency sources. Provider waiting, Python execution,
queues, compilation, checkout, IPC, host suspension, retries, transport and
conversion stay in client totals. Inclusive phases/profiles explain costs;
overlap is not additive and unobserved gaps stay explicit. Monty counters and
its telemetry adapter are not prerequisites, so no adapter dependencies or
local patch are proposed for subtraction. Retire the adjusted-client 500 µs
target without imposing it on total latency. Preserve separate historical
server criteria. Valid unprofiled totals may stand while required attribution
remains unfinished. Revision 5 and TASK-003 capture this explicit direction;
earlier retained-target statements above are historical.


## Profiling cleanup rationale — 2026-11-12

The plan drifted first into synthetic coverage validation and then into an
overcorrection that permitted missing client phases and relied on unproven
external-only profiles. Both miss the outcome: run the same benchmark and be
able to locate latency/CPU/allocation costs on either side. Reuse shared bench
recording and existing profiler owners, fill genuine language-boundary gaps,
remove duplicate/artificial collection, and prove production exclusion.

The implementor's claim of S4 machinery in production wheels remains to be
established against actual archives. Source review shows S4 collectors under
harness/test paths and normal Python/TS feature graphs without bench, while
shared recorder/timing definitions and server envelope state are not all gated.
That is reason to verify/fix compile-time and package isolation, not evidence
that a particular uninspected production wheel contains every S4 addition.
