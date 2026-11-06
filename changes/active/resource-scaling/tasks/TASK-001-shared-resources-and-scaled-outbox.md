---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-resource-scaling
spec_revision: 2
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, AC-001, AC-002, AC-003, AC-004, AC-005]
depends_on: []
parent_task:
remediates: []
---

# Shared startup resources and resource-scaled evidence writes

## Outcome and Value

Resolve Bifrost resource settings once during server startup and use that same
configuration throughout Bifrost. Connect evidence-write concurrency to its
effective CPU without reducing the current four-writer baseline.

A writer processes one ordered lane's batch, or slice. Each writer can encode,
submit and await Scribe acknowledgements for eight frame deliveries concurrently.
Four effective CPUs retain four writers, four lanes per tenant and up to 32
deliveries process-wide. Eight CPUs permit eight writers, eight lanes per tenant
and up to 64 deliveries. Independent work can use added CPU while one request's
evidence stays ordered. The task includes consumer wiring, tests, architecture
documentation and measured throughput evidence; these form one outcome.

## What Changes and What Must Stay True

The existing Bifrost resource owner resolves CPU, memory, headroom, scratch,
resource-derived defaults and existing overrides before dependent consumers
start. Scribe, Oracle, Forge, storage/cache, transport and the process outbox use
that configuration or its existing role capabilities. Server configuration still
owns parsing operator input; the Bifrost owner owns resolved resource sizing.
Consumers must not probe the host or create a second resource root. Existing
workload-sensitive sizing, reservations and pressure monitoring remain live.

Use `W = max(4, C)` writers and lanes, eight deliveries per writer, and the
derived process ceiling `8 × W`. Counts are immutable for the process lifetime.
Each delivery remains counted through encoding, submission and acknowledgement;
started blocking encoding remains counted even if its async waiter is cancelled.
Never apply the process ceiling independently to each writer.

Keep the current Scribe coordination/ingress/persistence formulas, replacing
their independent CPU probes with shared C only for omitted settings. Preserve
explicit values, accepted configuration keys and validation, including an
explicit value that happens to equal the old default. Do not infer omission by
comparing a parsed value with a host-dependent default. Preserve current WAL IO,
shard, database pool and queue settings. The shared runtime must remain independent
of Bifrost: supply resolved counts rather than importing Bifrost resource policy
into the generic outbox.

Preserve one process Scribe outbox; tenant separation; same-key/lane order;
identical retry batch identities; detail-before-summary; ACK-before-queue-handoff;
admission, shedding and loss accounting; and graceful shutdown. The separate
Verifier submission outbox keeps its existing geometry and semantics if generic
outbox interfaces change. Producer lanes are independent of durable Scribe shards.

Each process uses its own configuration, including a producer without local
Scribe and its receiving peer. Increasing concurrency can increase temporary
encoding memory and expose downstream saturation. Delivery counts are not byte
bounds; neither queue accounting nor sampled RSS proves an exact process-memory
ceiling. Preserve existing memory/headroom, scratch, Oracle and Forge policies.

Exclude new public product APIs or tuning knobs, SDK/wire/persisted contract
changes, dependencies/features, new resource roots or pools, live lane migration,
new byte reservations, larger queues, gateway behavior changes, SQL/file-reader
changes, cache-fill coalescing and code from the discarded branch.

## Approach

1. Extend the existing resource resolution/composition path with immutable
   execution settings and retain the distinction between omitted and explicit
   worker settings until that resolution completes.
2. Pass the shared configuration through production and test startup to every
   affected consumer. Preserve existing role derivations and report resolved
   settings and overrides through existing diagnostics.
3. Extend the existing generic outbox's lane configuration and Scribe sink's
   delivery accounting to implement the approved counts and real work lifetime.
   Update all affected constructors, sinks and test callers together.
4. Prove the changed behavior in the ordered TDD scenarios below, retaining
   existing correctness coverage and adding real server and SDK evidence.
5. Update the owning architecture and run focused owner checks. Then use the
   existing benchmark driver for baseline and actual-resource comparisons,
   recording completed evidence writes and correctness alongside resource use.

## Ordered Implementation Scenarios

### Scenario 1 — Startup sizing preserves the baseline and scales

**Behavior.** Inject complete resource observations into the existing owner.
Resolve the following counts before constructing any consumer:

| Effective C | Writers | Lanes per tenant | Deliveries per writer | Process ceiling |
|---|---|---|---|---|
| 1, 2, 4 | 4 | 4 | 8 | 32 |
| 8 | 8 | 8 | 8 | 64 |
| 16 | 16 | 16 | 8 | 128 |

An effective CPU cap lowers the observed CPU before these derivations. Zero,
unrepresentable derived counts and invalid observations refuse startup using
existing errors. Memory and scratch still use existing policy and validation.

**RED.** Add planned `resources::tests::startup_execution_counts_follow_resources`
using the existing injected owner path. Assert the matrix, cap precedence and
overflow refusal; the baseline lacks these resolved execution settings. Run
[resource sizing](#resource-sizing). Existing memory/scratch coverage runs as
regression proof rather than manufacturing a failing test for unchanged policy.

**GREEN.** Resolve the approved counts in the existing owner, with checked
arithmetic and existing startup errors. Keep one memory/scratch root and leave
unchanged role policies intact. Rerun this scenario.

**REFACTOR.** Reuse existing resource validation and role composition; remove
duplicate sizing rather than adding a sibling planner.

### Scenario 2 — Worker defaults and overrides use one CPU authority

**Behavior.** With observed CPU above a configured effective-C cap, omitted Scribe
coordination, ingress and persistence settings use capped C. Explicit worker
values survive unchanged, including values equal to what the old host default
would have produced. Invalid explicit values retain their errors. Startup
diagnostics describe the actual resolved settings and override provenance.

**RED.** Add planned
`config::tests::scribe_worker_resolution_preserves_explicit_settings`. Exercise
omitted, partially explicit, fully explicit and invalid configuration through
existing parsing and resolution; assert existing formulas against injected C.
Current deserialization materializes defaults from independent host probes and
loses omission provenance. Run [worker resolution](#worker-resolution).

**GREEN.** Preserve input provenance until the one resource resolution completes,
apply existing overrides and formulas there, and supply its outputs to dependent
executors. Remove consumer-side sizing probes, including alternate Scribe defaults.
Rerun this scenario and resource sizing.

**REFACTOR.** Consolidate duplicate CPU derivations at their existing owner. Keep
configuration syntax and error behavior; private input/resolved representations
remain implementation-owned.

### Scenario 3 — More lanes permit independent work without reordering

**Behavior.** Outboxes constructed with four or eight lanes use those fixed
counts for their entire lifetime. Gated writes show additional independent
lanes making progress, while later items in the same tenant/lane cannot overtake
an active or retrying slice. A retry keeps the original slice identity and items.
Tenants never share a slice. Existing sinks retain their prior geometry.

**RED.** Add planned
`outbox::tests::configured_lanes_preserve_order_retry_and_progress` beside existing
lane/retry tests. Use deterministic synchronization to demonstrate concurrent
independent lanes, blocked same-lane successors and an identical retry. Current
lane count is a sink-associated constant. Run [ordered lanes](#ordered-lanes).

**GREEN.** Let the existing outbox consume its immutable resolved lane count and
update its sink callers, preserving current batching, admission and scheduling.
Rerun this scenario and both startup scenarios.

**REFACTOR.** Reuse the current tenant/lane grouping and writer lifecycle. Do not
introduce a new scheduler, queue or ordering identity.

### Scenario 4 — Delivery limits preserve ACK overlap across writers

**Behavior.** Several writers with enough chunk work can each reach eight
deliveries while no writer exceeds eight and total real deliveries do not exceed
32 at C=4 or 64 at C=8. Hold acknowledgements to expose the live count: completion
of encoding alone must not free delivery capacity. Releasing acknowledgements
allows queued work and sibling lanes to progress without a stalled limiter.

**RED.** Add planned
`scribe_outbox::tests::delivery_limits_preserve_per_writer_ack_overlap` beside the
existing gated Scribe tests. Assert configured writer/lane capacity, per-writer
overlap, shared count and acknowledgement lifetime. The current constructors use
fixed writer/lane counts and lack the resolved shared ceiling. Run
[delivery overlap](#delivery-overlap).

**GREEN.** Wire the resolved settings into the existing sink and enforce their
derived count through acknowledgement. Preserve detail and summary phase order,
local canonical batches, peer IPC, frame identities and existing refusal handling.
Rerun this scenario and earlier scenarios.

**REFACTOR.** Keep the existing chunk delivery workflow and reuse its limit
ownership; avoid independent aggregate knobs or another executor.

### Scenario 5 — Cancellation cannot leave uncounted blocking work

**Behavior.** Park an encoding operation after its blocking work has started,
cancel the async waiter, and attempt more deliveries/retries. The parked work
still consumes process capacity. Repeated cancellation cannot exceed the real
work ceiling. Releasing the operation returns capacity exactly once; encoding
failure, successful ACK and retry/refusal paths also release their ownership.

**RED.** Add planned
`scribe_outbox::tests::cancelled_encoding_retains_delivery_capacity` using a
deterministic in-owner synchronization seam around the real encoding lifetime.
Do not substitute a cancelled async sleep for started blocking work. Current
started encoding can outlive its waiter without shared delivery ownership. Run
[cancellation lifetime](#cancellation-lifetime).

**GREEN.** Retain count ownership for the actual work that continues after
cancellation, and through ACK waiting when the delivery remains active. Preserve
existing shutdown deadlines and loss semantics. Rerun this scenario and delivery
overlap, then earlier scenarios.

**REFACTOR.** Simplify ownership and cleanup within the current sink. Add no
public cancellation API, new pool or byte-accounting system.

### Scenario 6 — Real startup consumers share the same configuration

**Behavior.** Boot combined and split-role processes from injected observations,
including a producer without Scribe. Assert one resource configuration per
process, consistent worker/outbox counts and diagnostics, existing role/cache/
transport derivations and independent receiving-peer resources. Exercise an
effective CPU cap and explicit worker overrides. Usage and pressure counters
continue changing without rebuilding configuration or multiplying roots.

**RED.** Add planned
`owner_inspection::startup_consumers_share_one_resource_configuration` in the
existing real-server target. Reuse the cluster's raw-observation injection and
owner inspection rather than constructing another engine graph. The current
startup mixes resource-owner CPU with independently defaulted workers and fixed
outbox geometry. Run [startup integration](#startup-integration).

**GREEN.** Complete production/test composition and all consumer wiring, exposing
only the narrow test inspection needed to verify the already-approved startup
diagnostics and ownership. Extend existing injection support for non-Oracle
targets where needed. Rerun this integration and preceding focused tests.

**REFACTOR.** Remove obsolete constructor defaults/probes and keep test startup on
the production policy path. Do not introduce a public resource-inspection API.

## Acceptance Criteria

| Spec obligations | Required observation and proof owner |
|---|---|
| REQ-001; AC-001 | Resource-owner unit tests establish the exact count matrix, CPU-cap precedence and invalid/overflow refusal. Real startup confirms those counts reach the outbox. |
| REQ-002; AC-002 | Sink tests establish per-writer/process ceilings, acknowledgement waits and retained blocking-work ownership after cancellation. No error path leaks or double-releases capacity. |
| REQ-003; AC-002 | Runtime/sink regressions preserve order, retries, phases, tenant slices, admission/loss and shutdown. The real SDK journey establishes local/peer evidence and ACK-before-handoff. |
| REQ-004 through REQ-006; AC-004 | Config tests and real startup integration establish one configuration across roles/targets, no independent sizing probe, correct override provenance and matching diagnostics. |
| REQ-005; AC-005 | Existing resource-owner memory/scratch regression tests establish unchanged headroom/caps/floors, shared accounting and bounded admission; larger observations do not multiply roots or queues. |
| REQ-003, REQ-004; AC-003 | Existing-driver experiments compare unchanged main/candidate on equal resources and the candidate on smaller/larger actual CPU resources. Require baseline non-regression within observed variation, and higher sustained ACK-completed throughput in a CPU-bound case, with reconciled loss/backlog and resource evidence. |

## Implementation Reference

Paths guide ownership, not private implementation layout or a file allowlist.

- **Resource composition:** `crates/vala/vala-bifrost-redux/src/resources.rs`,
  `BifrostRuntimeResources::{detect_with_transport_message_limit, from_snapshot_with_transport_message_limit, compose_roles}`,
  `ResourcePlan`, `shared_memory_cap`, `scratch_budget`, and existing injected
  `resources::tests`. They already own observations, one root and role handles.
  The gap is resolved execution geometry and consistent consumer sizing; extend
  this workflow, not a second planner or tenant/role resource pool.
- **Configuration and boot:** `crates/wyrd/wyrd-server/src/config.rs`,
  `ScribeRuntimeConfig`, its default/validation paths and
  `BifrostResourceConfig::policy`; `boot/mod.rs`, `compose_bifrost` and
  `forge_compaction_worker_config`. Production detects resources before role
  construction, but Scribe fields currently materialize host-derived defaults.
  Preserve explicitness until startup resolution. With
  `B = max(C.saturating_sub(2), 2)`, current defaults are coordination `max(C,2)`,
  ingress `max(B/3,1)` and persistence `max(B - max(B/3,1),1)`; explicit ingress
  does not redefine an omitted persistence setting's formula. WAL IO remains its
  existing configured/default value and shard floor.
- **Alternate Scribe defaults:**
  `crates/vala/vala-bifrost-redux/src/scribe/mod.rs::ScribeLaneConfig::{default, resolved}`
  also probes host parallelism independently. Close its callers against shared
  resource resolution rather than leaving an alternate production-equivalent
  path with different defaults. Preserve the existing execution-lane owners.
- **Role consumers already using resources:** the above boot path supplies
  Scribe pools and staging workers; `resources.rs` owns Oracle slots, grants,
  partitioning and Forge grants; `storage/policy.rs::BifrostStoragePolicy::resolve`
  derives cache sizing from managed memory; transport admission is composed by
  `BifrostRuntimeResources`. Preserve these whole workflows and their current
  policies. Updating startup configuration must not rebuild request-time roots.
- **Producer outbox:** `crates/wyrd/wyrd-server/src/scribe_outbox.rs`,
  `ScribeSink::{bounded_outbox, outbox, local_outbox, submit, deliver}`, its
  `OutboxSink` implementation and `ScribeRoute::select`. Writers/lanes currently
  use private constants; `submit` holds eight chunk futures per slice; `deliver`
  uses `spawn_blocking`, then awaits Scribe. Reuse the current route, batching,
  phase and retry pipeline while adding resolved counts and real-work ownership.
  Update boot, support constructors and direct test callers together.
- **Generic ordering:** `crates/shared/wyrd-runtime/src/outbox.rs`,
  `Outbox::new`, `OutboxSink::LANES`, `OutboxWriter` tenant/lane dispatch and its
  tests. Writers are already constructor-configured; lane count is currently
  static. Extend this owner to accept immutable lane sizing. Its other consumer,
  `verification/submissions.rs::SubmissionSink`, must keep its independent
  four-connection geometry, admission lane identity and ACK-gated handoff.
- **Real startup and journeys:** `crates/wyrd/wyrd-testing/src/server.rs`,
  `WyrdTestServerBuilder::start_with_resources`, already injects raw observations
  through the same resource owner before composing storage and roles.
  `bifrost/cluster.rs`, `BifrostClusterSpec::with_system_resources`, currently
  applies observations only to Oracle nodes; extend this existing seam as needed
  for Scribe-only resource proof, retaining per-process observations across
  restart. `tests/bifrost/server/{owner_inspection,verification_runtime}.rs`
  provide real ownership, SDK verification and queued ACK-gating precedents.
  Reuse `WyrdTestCluster`, `WyrdTestServer` and repository Card fixtures; add no
  second server bootstrap or new test binary.
- **Measurements:** `crates/wyrd/wyrd-testing/src/bin/capacity/gateway/run.rs`,
  `Integrity::between`, `Durable::between`, `settle`, and existing Gateway L1/L2
  runner paths already separate completed outbox work, loss and drain. Reuse
  these measurements and the existing record/report workflow. Pair startup
  diagnostics with artifacts so actual CPU/RAM and effective C/counts cannot be
  confused. No SDK, wire, generated output, migration or dependency change is
  expected; audit that boundary in the final diff.
- **Architecture:** update the resource policy in
  `architecture/bifrost-design.md` and its outbox description to distinguish one
  logical dispatcher from concurrent slice writers. Preserve memory, durability,
  audit, Oracle/Forge and operations overload contracts.

## Verification and Evidence

This is a mixed task. New executable behavior follows the ordered RED/GREEN/
REFACTOR scenarios. Unchanged memory/scratch and lifecycle behavior use regression
proof. Architecture prose, probe/consumer inventory and no-contract-change checks
use static diff review. Throughput uses opt-in experiments, not timing assertions
in unit tests.

The seven new selectors below are **planned**, not existing tests. Packages,
targets, runner and setup were confirmed from current manifests and `mise.toml`.
After adding each test, confirm its actual selector before its first run (RED
for changed behavior, regression for preserved behavior) and record any equivalent
private name correction. Run Cargo-backed checks sequentially. Use
`WYRD_LOG=info,wyrd_server=debug,vala_bifrost_redux=debug` while diagnosing failures.

### Resource sizing

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=resources::tests::startup_execution_counts_follow_resources)'
```

### Worker resolution

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=config::tests::scribe_worker_resolution_preserves_explicit_settings)'
```

### Ordered lanes

```bash
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=outbox::tests::configured_lanes_preserve_order_retry_and_progress)'
```

### Delivery overlap

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::delivery_limits_preserve_per_writer_ack_overlap)'
```

### Cancellation lifetime

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::cancelled_encoding_retains_delivery_capacity)'
```

### Startup integration

Before either real-server proof, build the server once, matching the existing
server-journey lane's setup dependency:

```bash
mise run build:server
```

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=owner_inspection::startup_consumers_share_one_resource_configuration)"'
```

### SDK evidence journey

Add planned `verification_runtime::scaled_outboxes_preserve_sdk_evidence` to the
existing server journey target. This is regression proof for preserved SDK
semantics under the new startup configuration, not a new SDK capability: no
manufactured RED is required for unchanged read-back behavior. The changed
startup and concurrency behavior has RED coverage in the preceding scenarios.

Drive real SDK verification/queued submission and Bifrost read-back through a
real server and repository Postgres, for baseline and expanded counts, on
combined and split Scribe/Oracle targets. Submit independent executions and read
back their exact inputs/results with correct attribution. Exercise a retry/ACK
hold, confirm nothing hands off before its required ACK, and ensure drain/shutdown
retains acknowledged evidence without duplication or tenant mixing. Preserve
public responses and negative admission behavior. Reuse checked-in Card fixtures,
SDK authoring paths and existing deterministic harness controls; do not replace
public calls with engine or SQL writes. Internal geometry assertions belong to
startup integration and unit tests, not a new product inspection surface.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=verification_runtime::scaled_outboxes_preserve_sdk_evidence)"'
```

### Affected owner checks

Run these after focused scenarios pass. The resource library owns all three
roles, so its library suite protects unchanged Oracle/Forge and shared-root
behavior. Runtime coverage protects generic admission/lifecycle and other sinks.
The server module selection includes configuration, composition, sink and
Verifier submission regressions; its wrapper supplies any Postgres-backed tests.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E "test(/^(config|boot|scribe_outbox|verification::submissions)::/)"'
mise run fmt:check
mise run lints
mise run check:deps
mise run check:tenant-isolation
mise exec -- git diff --check
```

Inspect changed items' rustdoc, configuration compatibility, tracked/untracked
diff and removed independent sizing probes. Do not add a bespoke source scanner
when tests and the ordinary source audit establish the property. Full language
surface and integrated capability suites run once at change review, not again
as this task's checklist.

### Throughput comparison

Use the existing Gateway driver, which exercises the evidence outbox and retains
completed-item/byte, loss, latency, resource and drain measurements:

```bash
mise run bench:l1:gateway
mise run bench:l2:gateway
```

L1 supplies the existing isolated-work repetitions; L2 supplies sustained work,
overload and recovery. Use the same declared workload, capture mode, payload and
measurement windows for every compared candidate. Retain the repeated samples,
driver/host health, raw counter windows and resolved settings with each record.
Confirm enough independent request keys and offered work reach the write path;
increase offered work only through existing driver controls, with equal settings
across a compared pair. Never generate unrelated synthetic host load.

First compare baseline main `5234961d51798581a026834cfffd5efaadc226b4` and the
candidate on equal resources. Then compare candidate runs on smaller/larger
actual CPU resources under otherwise controlled conditions, preferably crossing
the four-writer floor. Use provisioned machines or externally supplied resource
shapes; do not implement OS resource restrictions. Record physical/logical CPU,
effective C, actual memory, workers/lanes/ceilings, queue/dependency caps and the
existing memory policy. A configured-C sweep on one unrestricted host may
supplement this evidence but cannot replace actual-resource comparison.

Judge sustained completed outbox work during the window, separating subsequent
drain completions. Reconcile counts with retained evidence and every rejection/
loss counter: a written-item counter alone can include a terminally rejected
item, and HTTP request completion is not evidence durability. Baseline completed
throughput must not regress beyond repeated-run variation. Demonstrate higher
sustained completed throughput in a CPU-bound larger-resource case. Report
latency, backlog trend, loss, drain/reconciliation, CPU and sampled RSS together.
An evidenced downstream bottleneck explains flat throughput but does not pass
the scaling demonstration. Do not alter benchmark verdicts, thresholds or
reference records to obtain a pass; preserve existing integrity requirements.

If existing measurements cannot retain necessary evidence, extend only their
current owner with a focused measurement test before changing executable logic;
do not introduce another benchmark framework. Record exact commands, settings,
baseline/candidate identity and artifacts in the execution evidence. Benchmarks
run only during implementation qualification, never as part of this planning turn.

## Material Stop Conditions

- A required outcome would change the approved count policy, memory/scratch
  policy, tenant/auth/audit authority, ordering, retry identity, ACK or loss
  semantics, public/persisted contracts, resource ownership or dependency direction:
  stop affected work and request specification authority.
- A dependency, Cargo feature, migration, new resource pool/scheduler, new public
  setting, or broader performance change is necessary: establish why reuse cannot
  satisfy the spec before seeking authority; do not expand scope silently.
- Throughput measurement exposes a real regression or no CPU-bound scaling
  evidence: diagnose within this owner and preserve the acceptance requirement.
  If satisfying it requires a changed policy or excluded subsystem, return the
  evidence for a specification decision rather than weakening proof.
- Required actual-resource environments are unavailable: finish unaffected work,
  record the unavailable proof and external resource need, and do not claim
  implementation complete. An effective-C-only run is not equivalent evidence.

Private configuration representations, constructor wiring, lane API shape,
permit ownership, fixture synchronization, test names and equivalent focused
command corrections remain implementation-owned. They do not require a new
task or specification revision when approved behavior and proof stay intact.

## Authority Links

- [Approved specification, revision 2](../spec.md)
- [Repository rules](../../../../AGENTS.md)
- [Spec-driven development](../../../spec-driven-development.md)
- [Implementation execution](../../../implementation-execution.md)
- [Testing and named-test commands](../../../../TESTING.md#named-tests)
- [Bifrost admission/memory](../../../../architecture/bifrost-design.md#admission-and-memory)
- [Bifrost resource/failure invariants](../../../../architecture/bifrost-design.md#resource-and-failure-invariants)
- [Scribe shards](../../../../architecture/bifrost-design.md#shards-and-hierarchical-ownership)
- [Capacity and overload](../../../../architecture/operations/reliability-and-recovery.md#capacity-and-overload)
- [Security principles](../../../../architecture/wyrd-security-posture.md#security-principles)
- [Rust ownership](../../../../agent_docs/rust.md)
- [Benchmark method](../../../../benchmarks/README.md)

## Execution Evidence

Candidate commits on `change/resource-scaling`: `d7c7f498c` (shared plan:
worker counts, outbox geometry), `9980d062c` (outbox lanes, process delivery
permits, startup diagnostic, harness on the production policy path, journeys,
architecture), and the follow-up lint/journey commit. Baseline main
`5234961d51798581a026834cfffd5efaadc226b4`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-001 startup counts | `resources.rs` `OutboxExecution::resolve`, `ScribeWorkers::resolve`, `ResourcePlan.{outbox,scribe_workers}` from `from_snapshot` | `resources::tests::startup_execution_counts_follow_resources` (C=1/2/4→4/4/32, 8→8/8/64, 16→16/16/128, cap, overflow `InvalidPlan`, zero refused) | PASS |
| AC-002 concurrency/correctness | `outbox.rs` `OutboxSink::lanes`, `OutboxWriter.lanes`; `scribe_outbox.rs` `ScribeSink::deliver` process permit held encode→ACK and moved into the blocking closure | `outbox::tests::configured_lanes_preserve_order_retry_and_progress` (lanes 4, 8); `scribe_outbox::tests::delivery_limits_preserve_per_writer_ack_overlap` (C=4, 8); `scribe_outbox::tests::cancelled_encoding_retains_delivery_capacity`; mutation RED: early permit drop fails the overlap test, uncounted blocking encode fails both; 21/21 `wyrd-runtime outbox::`, 143/143 wyrd-server `config|boot|scribe_outbox|verification::submissions` with Postgres | PASS |
| AC-003 throughput | existing `capacity l1/l2 gateway` driver, `--allow-dirty` records under `target/benchmarks/` | see Throughput below: L1 regresses beyond run variation on this host; no provisioned smaller/larger CPU environment | FAIL |
| AC-004 one configuration | `config.rs` `BifrostRuntimeConfig::resource_policy`, `ScribeRuntimeConfig::worker_overrides`; `scribe/mod.rs` `ScribeLaneConfig::for_resources` (host probe removed); `boot/mod.rs` plan-driven pools, outbox and `Bifrost resource configuration resolved` diagnostic; test server uses `resource_policy` | `config::tests::scribe_worker_resolution_preserves_explicit_settings`; `owner_inspection::startup_consumers_share_one_resource_configuration` (Oracle-only producer at 16 CPUs and Scribe peer at 4 resolve their own plans; combined node with cap 8 over observed 32 and explicit ingress 5; outbox sink runs the plan; Scribe/Oracle share the root) | PASS |
| AC-005 memory/scratch | unchanged `shared_memory_cap`/`scratch_budget` | 919/919 `vala-bifrost-redux --lib` with Postgres | PASS |
| SDK evidence journey | — | `verification_runtime::scaled_outboxes_preserve_sdk_evidence` (combined C=4, C=16, split Oracle/Scribe C=16; 40 queued executions each, unauthorized enqueue refused and not queued, exact attributed results after drain) | PASS |

Checks: `mise run fmt:check`, `mise run lints`, `check:deps`,
`check:tenant-isolation`, `git diff --check` pass.

Diagnosis (journey negative): **Symptom** `scaled_outboxes_preserve_sdk_evidence`
read `202 {"execution_id":…}` for an unknown Verifier UID. **Cause** the enqueue
route accepts and the queued path judges later; the test's assumption was wrong.
**Fix site** the test: it now uses an existing admission refusal (caller
without the runner role).

Non-goals: no new public setting, dependency, feature, migration, SDK or wire
change; queue capacities, pools, WAL IO and shards unchanged;
`SubmissionSink` keeps four lanes. Unrelated working-tree entries
(`changes/active/benchmark-scaling/*` deletions, `changes/active/gateway-latency-capture/`)
were not touched.

### Throughput

Host: AMD Ryzen 9 9950X, 32 logical CPU, 91 GiB, Linux; the only host
available. Effective C = 32 on both: baseline 4 writers/4 lanes/32 deliveries;
candidate 32/32/256; Scribe workers 32/10/20 on both; shards 4.

`mise run bench:l1:gateway -- --allow-dirty`, both VALID, median (min..max) of 3:

| step | baseline req/s | candidate req/s | server cores b→c | peak RSS b→c |
|---|---|---|---|---|
| buffered-1kib-metadata c1 | 4453 (3815..4515) | 4166 (4040..4181) | 1.18→2.13 | 867→2699 MiB |
| buffered-1kib-metadata c64 | 53368 (51131..53408) | 49372 (48750..50705) | 19.2→20.2 | 2308→2579 MiB |
| streamed-1kib-metadata c64 | 1536 | 1536 | 1.49→2.67 | 1446→3367 MiB |
| buffered-64kib-metadata c64 | 18269 (16791..18763) | 17018 (16986..18335) | 20.1→20.3 | 2321→3227 MiB |
| buffered-64kib-payload c64 | 4783, p99 169 ms | 5110, p99 95 ms (p95 22→46) | 11.1→16.8 | 8882→14053 MiB |

No errors, loss, shed or undrained captures in either. Records:
`target/benchmarks/gateway/overhead/20261106T175910Z-2714244aa.json` (candidate);
baseline `20261106T175550Z-5234961d5.json` in a scratch baseline worktree.

Diagnosis (independent read-only diagnostician): **Symptom** candidate
throughput below baseline's range on linger-bound steps, with 1.8× CPU and
2.3× RSS at an equal, upstream-paced rate. **Evidence** each lane lingers and
slices independently (`outbox.rs` `is_ready`/`lingered`/`take`), captures
spread evenly by request id over 32 lanes instead of 4, so each slice carries
~1/8 the rows (streamed c64: ~19 → ~2.4 rows per slice); every slice pays
fixed frame build, `spawn_blocking` encode, Scribe ingress/WAL and one memtable
`RecordBatch`. Worker pools and shards are identical. **Cause** batching
dilution inherent in "a slice is one lane's batch" once lanes grow with C;
the byte-bound payload step gains from parallelism. **Fix site** `OutboxWriter`
batching; the fix that restores batching (one slice covering several idle
lanes of a tenant, retried by its lane set) changes the approved definition of
a slice, so it is a specification decision, not an implementation choice.
