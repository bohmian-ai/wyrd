---
id: SPEC-benchmark-latency-diagnosis
revision: 7
status: approved
---

# Symmetric benchmark evidence and Gateway/Verification latency improvements

## Problem and proposed outcome

Wyrd's capacity experiments have two latency collection paths: precise HDR
histograms used by the main benchmark judgments, and production Prometheus
histograms still used by some diagnostic means and component reports. Gateway
phases were absent from the benchmark distributions. This makes attribution
incomplete and leaves readers comparing measurements with different precision
and populations.

Install the benchmark recorder as the sole metrics recorder in each benchmarked
server process across Bifrost, Verification and Gateway, at L1 and L2. It owns
latency distributions, diagnostic histograms, counters and gauges, and supplies
the benchmark snapshots consumed by the driver. No Prometheus recorder or
exporter is installed in benchmark mode. Give this owner a clear benchmark-specific
home outside `wyrd_telemetry` and activate it only for explicit benchmarking.
Ordinary deployments install the production Prometheus recorder instead and
retain their existing monitoring behavior.

Reuse the existing standard `metrics` instrumentation and HDR benchmark recording
where they fit. This is a choice between two process backends at startup, not a
Prometheus recorder with its timing observations filtered out. The benchmark
backend must not maintain a second independent timing registry for the same
observations. CPU and heap profilers remain separate diagnostic tools; neither
is a competing metrics recorder.

Then use the existing Gateway records, profiles and source to explain the c64
capture hot path and produce a concrete optimization proposal. Where the evidence
cannot establish a cause, name the uncertainty and the smallest measurement
that would distinguish the competing explanations. A slow background Scribe write
is not, by itself, a slow Gateway response.

The maintainer extended this packet to implement the four identified hot-path
optimizations in TASK-003, after repairing and qualifying the benchmark recorder.
The task first restores the saved pre-task baseline with the corrected recorder
and canonical diagnostic reporting, then improves capture draining, metadata
response usage extraction, post-answer payload materialization and foreground audit
construction. These are required parts of one task, not deferred proposals or a
reason to repeat the general diagnosis. Preserve all existing product semantics.
The desired c64
p50s are client completion below 1 ms and Gateway-added latency below 500 us;
these are investigation objectives, not new production SLOs or changes to the
existing benchmark p99 judgments.

The maintainer further requested one TASK-004 encompassing the remaining client,
fixture and JSON/transport work. It reduces SDK request replay and per-call
bookkeeping, Gateway driver and fake-upstream materialization, native request
ownership, repeated native response validation, and provider body accumulation.
Fresh allocation evidence and separately attributable production/fixture
comparisons are required. All work stays in this task; it is not a research-only
handoff, and the existing latency objectives do not become guaranteed outcomes.

TASK-005 extends the same measurement contract to client processes and the full
controlled Gateway and Verification paths. Client and server use the same
benchmark recorder under explicit runtime activation and the same CPU/heap
profiling workflow. It includes client/fixture timing, local work inside provider
exchanges, request handoff/waits and model-backed Verification against a local
provider. Complete the supported small optimizations identified by those profiles
inside TASK-005; instrumentation and a proposal-only report do not complete it.

The model-check profile exposed a product cost: reading a result reconstructs
the executable inline Agent and compiles its Prompt, although the caller needs
the judgment. TASK-005 now also separates the compact decision from full retained
execution evidence. For example, a caller receives the failed requirement,
reason, citation and custom score immediately; it explicitly reads execution
details to inspect the judge's Prompt or intermediate tool calls. The server
continues recording those details. Neither result nor detail decoding compiles
a Prompt. This is an intentional public contract change across all surfaces,
not a fixture optimization or permission to discard retained evidence.

Full details are not guaranteed durable when a verdict returns. Keep the useful
decision self-contained and preserve asynchronous recording, existing retention
and read authority. The costs are a deliberate SDK migration and an explicit
network read for diagnostics; no automatic detail fetch or new durability wait
is introduced.

## Required behavior

Revision 7 adds TASK-006: local Gateway quota/usage reservations with asynchronous
PostgreSQL persistence, plus lower Agent check latency through reused schema
preparation and evidence-backed execution fixes. Global quota enforcement is
approximate, including monetary budgets. Pending usage is memory-owned and may
be lost on abrupt process death; no crash-safe local ledger is required. Cold,
unhealthy or full usage state refuses until background recovery succeeds.
These explicit trade-offs remove per-call database coordination without
weakening authentication, permissions, tenant separation or model-answer
validation. TASK-006 does not require TDD; it still requires focused regression,
integration and honest performance evidence.

- **REQ-001 — Sole benchmark recorder.** The benchmark backend is the only
  process metrics recorder in benchmark mode. All benchmark timing distributions,
  diagnostic histograms, counters, gauges and resource-metric observations flow
  through that owner and its snapshots. Reuse existing standard `metrics`
  instrumentation and HDR distribution/interval machinery; do not install a
  Prometheus recorder/exporter, retain a filtered Prometheus recorder for health
  metrics, or run a parallel independent timing registry. One semantic latency
  observation is counted once even where existing instrumentation has both metric
  and explicit timing calls. Client and server measurements remain separately
  identified with process-local clocks. OS CPU and RSS sampling remains an
  external measurement source, not a second application metrics recorder.
- **REQ-002 — Exclusive startup selection.** Benchmark mode selects its recorder
  before metric observations are registered. Ordinary deployments install the
  production recorder and neither activate nor accumulate benchmark data. There
  is no live recorder switch or mixed-backend fallback. Explicit benchmark
  activation preserves the existing loopback-only access restriction. Production
  monitoring remains unchanged outside that mode. Names and documentation clearly
  distinguish benchmark recording from tracing and OTLP export.
- **REQ-003 — Trustworthy evidence.** Every distribution identifies its boundary,
  unit, precision, outcome and population. Windows reconcile counts and handle
  late completions, replaced processes, invalid labels, overflow and missing or
  negative components explicitly. Compute overhead within each request before
  aggregation. Never subtract percentiles, sum overlapping phase percentiles or
  average percentiles into a combined population. Benchmark classification remains
  bounded and conveys no authority.
- **REQ-004 — Complete consumer closure.** Migrate Bifrost, Verification and Gateway
  consumers together with their correctness bookkeeping to the benchmark-owned
  snapshots. Benchmark execution must not depend on Prometheus exposition or its
  parser for latency, counts, backlog, health or resource metrics. Preserve the
  actual meaning and population of counts that prove capture or acknowledgement,
  including histogram observation counts where appropriate. Preserve useful
  diagnostic coverage or document and justify its replacement; migration must not
  silently turn observations into zeros, erase diagnostic classes or weaken
  validity checks. Production metrics contract tests may continue using their
  production backend and parser.
- **REQ-005 — Gateway attribution.** Distinguish request admission, capture-slot
  reservation waiting, provider exchanges, answer handoff and handler resumption,
  capture projection/staging, outbox scheduling, and durable submission. Explain
  which work is on the response path, which shares its execution resources, and
  which occurs only after handoff. CPU samples establish sampled CPU work, not
  off-CPU waiting. Do not label wall time as CPU time.
- **REQ-006 — Client boundary.** Explain what the driver includes in client
  completion, including response reception and validation, and what the server
  excludes as provider exchange time. Identify local transport and scheduling
  effects that those exclusions can contain. Evaluate both p50 objectives
  separately for the existing 1 KiB metadata, 64 KiB metadata and 64 KiB payload
  cases at c64, using c1 and streaming evidence as relevant controls.
- **REQ-007 — Reuse evidence first.** Analyze the available baseline and candidate
  records before proposing any new experiment. Do not rerun the baseline. Keep
  profiled and unprofiled results distinct. Preserve raw evidence and record
  source identity, dirty diff, build identity, workload, settings and profiling
  mode. Symbolized CPU conclusions require the matching recorded binary or
  matching symbols and an identified sample interval; whole-run samples must not
  be presented as one step's profile.
- **REQ-008 — Concrete proposal.** Produce a ranked explanation separating measured
  facts, source-proven mechanisms, hypotheses and unresolved gaps. For each
  material gap, specify the existing tool or smallest additional observation,
  workload, distinguishing result and stopping condition. Propose corrections
  only for supported causes, at their existing owners, with the expected effect,
  correctness risks and verification. A gap requiring measurement is not an
  implemented performance fix.
- **REQ-009 — Recover the workspace.** Inventory the uncommitted recorder and
  instrumentation changes against this specification before further execution.
  Treat them as an unfinished candidate, not approved design or completed proof.
  Retain, rework or remove candidate changes according to the approved outcomes;
  preserve pre-existing work and record the disposition. Previously interrupted
  checks do not count as passes.
- **REQ-010 — Qualify the measurement path.** Remove process-wide update
  serialization and unnecessary registration work in the sole recorder while
  preserving coherent timing groups and snapshots. Demonstrate performance
  recovery against saved unprofiled pre-task evidence with full instrumentation,
  and measure added instrumentation cost. Correct counts or a `VALID` verdict
  alone do not establish acceptable recording overhead. Do not rerun historical
  baselines or weaken observations to claim recovery.
- **REQ-011 — Canonical diagnostic reporting.** Persist bounded phase diagnostics
  and their populations in the existing experiment record. Fresh runs and saved
  reports use the same terminal renderer, retain each repetition and expose
  missing/invalid evidence and overlap. Regeneration works without raw artifact
  directories. Existing supported records remain readable. Document the common
  reporting rule for implementors rather than require custom extraction scripts.
- **REQ-012 — Implement the four identified optimizations.** After recorder
  recovery is confirmed, TASK-003 implements all four changes at their existing
  owners: remove avoidable capture-drain copying/bookkeeping; avoid full owned
  response JSON solely for metadata-mode usage extraction; reduce repeated
  post-answer payload materialization; and reduce foreground audit construction
  work. Preserve response/usage behavior, redaction, capture content and counts,
  byte accounting, audit attribution, retries and acknowledgements. Authentication
  caching, new execution policies and incremental retry/capacity redesign are not
  selected. Verify correctness at the affected owners and through existing real
  server journeys, then compare the optimized candidate against the corrected
  recorder-only milestone with unchanged instrumentation and workload. The task
  is not complete at recorder recovery and does not defer these optimizations.
- **REQ-013 — Efficient SDK calls.** TASK-004 removes serialized-request copying
  across initial/replayed HTTP attempts, whole-client timeout wrappers used by
  Verify, and avoidable binding-lookup and request-assembly allocations. Preserve
  exact bodies, effective deadlines, retry/idempotency, invocation/request identity,
  token exchange/refresh and secret handling. Reuse existing pools and caches;
  no new authentication cache, caller authority or SDK wire surface is selected.
- **REQ-014 — Efficient fixture clients with unchanged checks.** TASK-004 avoids
  full owned JSON trees in Gateway buffered-answer validation and the fake
  upstream's request selection, and reuses invariant request setup. Preserve
  decoded-value checks, duplicate-key semantics, malformed-input rejection,
  response bytes, pacing, full-receipt-and-validation completion, and dynamic
  case/phase/caller headers. Declare the changed fixture method. Gateway's timed
  caller is direct HTTP; Verification's timed caller uses the SDK. Improvements
  must be attributed to the implementation actually exercised.
- **REQ-015 — Reduce native JSON and response-buffer work.** TASK-004 avoids
  the owned native metadata request tree where equivalent raw/borrowed handling
  is possible, consolidates Gateway-native response validation/adaptation, and
  avoids unconditional copying of single response chunks. Keep owned fallback
  for consumers/forms needing it. Preserve typed validation, canonical token
  bounds, numeric/depth/duplicate/escape behavior, model rewriting, extension
  fields, capture/redaction, provider refusals, UTF-8 behavior, decoded-body limits,
  multipart/media/streaming behavior and retry ownership. Standalone public
  Skald raw-response APIs remain unchanged. Internal representation changes at
  existing owners are allowed; unchecked JSON construction or a parallel parser,
  transport or lifecycle owner is not selected.
- **REQ-016 — Current allocation and attributable performance proof.** TASK-004
  obtains current server allocation evidence rather than treating the historical
  702 KiB/call as current or as copied-byte volume. Compare production changes
  under unchanged fixture behavior, then compare fixture changes with the exact
  same retained optimized server binary. Keep profiling separate from unprofiled
  judgments and record component binary/source lineage, workload methods and
  all repetitions. Server profiles do not establish driver allocation. Historical
  baselines remain immutable and are not rebuilt; bounded fresh current-reference
  measurements for this task are distinct from rerunning historical baselines.
- **REQ-017 — Client/server recorder parity.** In explicit benchmark mode, each
  participating process installs the existing sole `wyrd-benchmark` backend before
  metric registration. Benchmark client timing uses the same histogram precision,
  snapshots and validity machinery as the server. Ordinary clients install no
  recorder/subscriber, start no timers and accumulate no benchmark observations.
  Installation belongs to process startup, never a client constructor. Reuse
  `WYRD_METRICS_BENCHMARK_SNAPSHOT=1`; no new metrics feature or backend is selected.
  Provide the required benchmark tracing registry even with ordinary log output off.
- **REQ-018 — Full controlled-path evidence.** Measure logical client calls and
  HTTP attempts, SDK preparation/auth/serialization/send-to-headers/body/decode,
  direct Gateway caller and local-provider work, server ingress/governance/dispatch,
  provider headers/body, answer resumption, Verification hosted setup and background
  capture/draining. Record success/error/cancellation and late cohorts explicitly.
  Provider exchange is an interval containing Wyrd-controlled work, not proof of
  external provider compute. Preserve original total/residual/queue boundaries;
  new actual-dispatch and handoff observations have distinct meanings. Driver and
  server clocks/populations remain separate; nested phases are labelled and not summed.
- **REQ-019 — Symmetric CPU and heap profiling.** The existing profiling command
  profiles driver and server with explicit process roles and retained matching
  binaries. Bare `--profile` keeps combined behavior; optional values `cpu`,
  `heap`, and `both` select the collectors symmetrically. CPU-only starts no heap
  sampler; heap-only starts no CPU sampler. Extract the existing heap sampler
  into `wyrd-benchmark` behind one earned, default-off `heap` feature. It excludes
  sampler/unwinding/pprof support from the ordinary client feature path, without
  forbidding unrelated existing dependencies. The client adds only the plain
  narrow recorder dependency; server's existing `bench` feature and tooling enable
  heap support. Preserve binary-owned allocators and server memory charges; no
  test-tooling crate enters production dependencies. Keep driver out of server
  resource accounting. Preserve historical schema-3/server-only records accurately.
- **REQ-020 — Cover both Verification paths.** Keep current assertion controls and
  add separately declared `direct-verify-model-c1`/`direct-verify-model-c64`
  diagnostics through the same WyrdState SDK and Card-bound runtime account.
  One Agent check calls a deterministic local Gateway provider and returns a valid
  cited verdict with usage; no external provider/credential is required. Reconcile
  actual provider calls, accepted authority, result/citation counts and retained
  evidence. Do not apply assertion-only latency judgments to new model diagnostics.
- **REQ-021 — Deliver supported small optimizations.** Within TASK-005 implement
  canonical borrowed-string comparison without redundant formatting and remove
  unnecessary Vertex access-token HTTP client construction. Use full-path CPU,
  phase and allocation evidence to select and implement safe owner-local reductions
  in tracing bookkeeping, unused Verification hosted setup and capture-drain work.
  A candidate lacking removable work must have concrete counterevidence; supported
  changes are not deferred. Preserve all trace/tenant/authorization/capture/retry
  semantics. New auth caches, schedulers, retry/capacity redesign or suppressed
  required telemetry are outside this scope. Report measured benefits honestly.
- **REQ-022 — Qualify instrumentation and improvements.** Retain current-reference,
  measurement-complete and optimized milestones. Qualify added observation cost
  before crediting optimizations. Use CPU-only profiles for CPU ranking and separate
  per-process heap evidence for allocation. Keep exact case/window scope or explicit
  clock uncertainty, process identities, binaries and all repetitions. Persist
  client/server readings in the existing canonical reports; missing data is never
  zero. Do not substitute residual for all Wyrd work, CPU shares for latency, or
  a mean-throughput calculation for a median hardware limit.

- **REQ-023 — Compact decision contract.** `VerificationResult` keeps its existing
  top-level `execution_id`, optional `run_id`, exact `verifier`, `subject`, optional
  `deployment_environment`, and `outcome`. The outcome contains only `verdict` and
  `requirements`. Each requirement retains `id`, optional `step`, `statement`,
  `outcome`, `reason`, optional `actual`, `citations`, optional custom `data`, and
  optional check-specific `detail` (including existing drift statistics). Full
  `check` and execution `provenance`, outcome `steps`, and outcome `evidence` are
  absent, not null placeholders. Direct execution, status, wait and typed reads
  use this same compact contract; the meaning and validation of all retained
  fields and custom data are unchanged. Citations still identify original slots
  and locators; they do not promise that input bodies are echoed or fetchable.
- **REQ-024 — Explicit descriptive details.** Add the read operation
  `GET /v1/verification/executions/{execution_id}/details`, returning
  `VerificationDetails` with the same execution/Run/Verifier/subject/environment
  identity and full outcome: verdict, requirements including exact authored
  checks and execution provenance, steps and pinned evidence declarations.
  Expose it through the existing SDK `Execution` handle as `details` (and its
  ordinary typed-data sibling), and MCP `verification.get_execution_details`.
  The details preserve inline configuration even without standalone Agent or
  Prompt Cards. They describe what ran; neither client decoding nor retained
  server readback compiles, renders, hydrates or executes a Prompt. Structural
  decoding and stored-data integrity checks remain; authoring/execution validation
  and validation of model answers against their declared schema remain mandatory.
  No wire-supplied trusted/validated flag or general schema cache is introduced.
  REQ-032 permits narrowly owned immutable execution-preparation reuse; it does
  not make descriptive reads compile or execute a Prompt.
- **REQ-025 — Preserve evidence ownership.** Keep complete existing canonical
  summary, output and input records, their shapes, selected result identity,
  provenance, capture rules and retention. Compact response projection must not
  remove check configuration, inputs or steps from storage. Read both projections
  through the existing execution reader and authenticated Bifrost workflow;
  introduce no duplicate store, ledger or execution orchestration. Previously
  retained records remain readable as descriptive details without rejudgment.
- **REQ-026 — Honest detail availability and authority.** Details require the
  same tenant, Card/read and Bifrost authority as current full execution readback;
  `verifier:run` does not confer read access. Reuse existing permission/audit and
  non-disclosure rules. If no complete visible retained detail can be assembled,
  including an unknown execution, not-yet-visible records or expired/lost records,
  return `404 WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND` under the existing read
  rules, never an empty successful detail or an invented pending/durable claim.
  Do not turn permission or query failures into successful empty data. Known
  queued status and its optional result retain their existing lifecycle behavior.
  Direct calls never wait for durable detail publication; completed verdicts and
  earlier responses are unchanged by failed detail reads. SDK properties and
  default execution/status/wait perform no extra detail request. No new retention
  promise, persistence acknowledgement or availability tracking service is added.
- **REQ-027 — Coherent public migration and proof.** Update wire schemas, served
  OpenAPI/MCP descriptions, Rust/Python/TypeScript SDKs, authored stubs, examples,
  and affected journeys together. Configuration/step/input consumers explicitly
  request details; ordinary outcome handling does not. There is one compact
  default contract, no legacy alias, hidden full mode or compatibility fallback.
  Measure final response bytes, client/server CPU, allocation, latency and
  throughput with symmetric TASK-005 collection. Preserve earlier evidence and
  disclose the changed response representation; no new performance claim is
  established by old profiles or by changing the benchmark's validation boundary.

- **REQ-028 — Local approximate quota admission.** TASK-006 extends the existing
  process Gateway cache/lifecycle to hold tenant policy, refreshed shared usage
  and local pending reservations. Request, token, concurrency and spending-budget
  checks reserve atomically within a server before dispatch, accounting for the
  existing applicable subject/target, window/period, retry/fallback exposure and
  unknown-cost rules. Admission uses that local view and does not await PostgreSQL
  reads, writes or cache fills. Accounting does not delay a buffered answer or
  stream handoff. Explicit authentication/token-exchange and administrative
  operations are outside this quota-path IO guarantee.
- **REQ-029 — Approximate global enforcement.** Cached shared usage and local
  reservations replace strict cross-replica serialization for all configured
  Gateway quotas, including monetary budgets. Servers may exceed a configured
  cluster total before their usage becomes mutually visible; a hard global
  ceiling or numerical maximum overrun is not promised. Local admission still
  enforces its current policy/view and cannot double-spend local headroom.
  Existing applicable-limit/budget refusal codes remain when the usable local
  view is exhausted. Approximate quotas are not authorization controls.
- **REQ-030 — Background synchronization and refusal.** Cache loading, shared
  usage refresh and persistence run through bounded Gateway-owned background
  work. A missing usable policy/usage snapshot, loss of refresh health or a full
  pending-accounting buffer refuses quota-covered work before dispatch with
  `503 WYRD_SERVER_503_SERVICE_UNAVAILABLE`, preserving existing dialect mapping.
  An unknown policy cannot be assumed to contain no quotas. Requests never
  substitute a synchronous database query or silently admit unlimited work.
  Recovery resumes admission only after a usable shared baseline is established.
  Reuse notification/listener health and generation-fencing behavior; bound
  background IO and stale-state detection, retaining the existing listener
  liveness bound rather than claiming instantaneous notification/revocation.
  Private batching, refresh scheduling and capacity sizes remain implementation
  choices with observable boundedness and failure tests.
- **REQ-031 — Memory-owned usage and replay-safe persistence.** Pending usage
  and reservations may live only in memory. Abrupt process death or an expired
  shutdown deadline may lose usage not yet committed to PostgreSQL; this is an
  accepted additional source of undercount/overshoot, not a durable-accounting
  promise. Do not add a local WAL, durable queue or separate coordination service.
  During a live process, retain accepted usage until committed or the approved
  shutdown boundary, refuse new work before its bounded pending capacity is
  exceeded, and drain through the existing tracked lifecycle. Persist server
  contributions without overwriting other servers' usage. A refresh must neither
  erase pending reservations nor double-count committed local contributions.
  Retried/uncertain commits must not repeat token deltas, costs or releases.
  Cancelled/failed calls settle the applicable actual/bounded exposure; local
  concurrency releases only when execution ends, including stream termination.
  Restart loads committed shared usage in the background before admission; it
  cannot recover usage already lost from another process's memory.
  Extend existing PostgreSQL queries/migrations only as needed, preserving tenant
  RLS, committed historical usage, domain identity and PostgreSQL-owned
  coordination time. Use existing deployment migration/drain rules; no new
  replica lease, distributed reservation, reconciliation service or rollout
  protocol is selected. Do not reprice historical entries.
- **REQ-032 — Reuse exact schema preparation.** Repeated warm Agent checks with
  the same effective schema/validation settings reuse immutable meta-validation
  and compiled validators through an existing owner whose lifetime spans calls.
  Bounded eviction may require preparation again. Rendering, reachable-slot
  narrowing and schema-changing hooks must select the matching preparation;
  changed schemas/settings cannot reuse stale validation. Continue validating
  model answers against the outgoing schema and the declared Verdict contract
  where they differ, decoding typed Verdicts, enforcing custom-data bounds and
  checking original-evidence citations. Code checks retain their validation.
  Do not treat a wire/decoded output map as proof it was validated, introduce a
  public trusted flag, or retain caller authority, evidence/media, mutable
  conversations, tools or workspaces in reusable preparation. No general global
  schema registry or new remote schema retrieval behavior is selected.
- **REQ-033 — Complete Agent latency remediation.** Recover the saved Gateway
  phase/wait distributions already present in Verification's raw snapshots and
  persist them through the existing canonical reports. Distinguish provider
  headers/body, task dispatch/resumption, local preparation/validation and
  post-answer work. Remove repeated schema preparation and implement supported
  owner-local reductions in the remaining Agent path, preserving retries,
  deadlines, structured-output repair and retained evidence. Rejected candidates
  require source/profile counterevidence; a research-only handoff is incomplete.
  Neither PostgreSQL removal nor CPU shares explain the existing no-quota
  fixture's whole elapsed Agent interval. Existing benchmark authorization and
  comparable milestone/role/population requirements remain in force.

## Invariants and boundaries

- **INV-001:** authentication, permissions, tenant separation, credential
  scrubbing, audit behavior, governance, provider behavior, stable errors and
  evidence attribution do not change. Response contents remain unchanged except
  for the explicit compact/detail Verification contract in REQ-023–027. This
  exception changes presentation and decoding, never judgment or stored evidence.
  REQ-028–031 explicitly replace strict Gateway quota/settlement coordination
  with approximate local enforcement and memory-owned asynchronous persistence;
  permission decisions and tenant boundaries are unchanged.
- **INV-002:** capture reservation remains before charge/dispatch, bounded by the
  existing deadline, with the existing retryable refusal. Reserved captures are
  never shed. Projection failures never change an already answered call.
- **INV-003:** recording and optimizations do not change cancellation, settlement, accepted-work
  ownership, retry, durable acknowledgement, or resource-admission semantics,
  except for the explicit Gateway quota/accounting changes in REQ-028–031.
  Capture and Verification evidence acknowledgment/loss rules are unchanged.
- **INV-004:** historical records remain immutable and readable. Changed
  measurement settings are declared; results from different instrumentation or
  workloads are not represented as a controlled performance improvement.

The benchmark recorder is a small shared instrumentation owner, not a test
harness dependency, tracing exporter or new durable service. Reuse the existing
monotonic timers, standard metrics instrumentation, HDR distributions, snapshot
access and experiment driver. Any required counters/gauges snapshot fields belong
to this same owner; no separate collection service is introduced.
Its exact internal module or crate placement remains an implementation choice
subject to repository dependency boundaries. `wyrd-benchmark` is the current
uncommitted candidate, not a mandated name.

Recording overhead, synchronization, cardinality and profile overhead are costs
to account for. New timing observations must not be assumed free or proposed as
latency fixes. No new scheduler, cache, allocator, transport, memory-accounting
framework or admission policy is selected here except the Gateway-local usage/
reservation cache and narrow immutable execution preparation in REQ-028–032.
The only new product read workflow
is explicit execution-detail retrieval under REQ-024–027.

## Execution boundary

Creating and reviewing this packet performs source and existing-artifact research
only. It runs no benchmarks, starts no release builds and changes no implementation
code. Specification approval permits task planning, not automatic execution.

The maintainer explicitly waived TDD for TASK-006 on 2026-11-11. That task may
implement fixes directly, followed by focused regression, integration, negative
and performance/ownership evidence; no failing-first test or RED/GREEN/REFACTOR
cycle is required. The waiver does not remove correctness coverage, relax
assertions, authorize experiments or change other tasks' proof requirements.

After task execution is explicitly requested, ordinary implementation checks may
verify recording, reporting and the four scoped optimizations. Any proposed new benchmark experiment must first be
presented as a concrete, bounded measurement with its evidence gap, cases,
profiling mode and interpretation, and receive explicit maintainer authorization.
A full benchmark suite is not a default prerequisite for diagnosis. Existing
baselines are reused throughout.

## Acceptance and proof

| Criterion | Obligations | Observable proof |
|---|---|---|
| **AC-023 — Local quota path** | REQ-028–030 | With usable local state, pause background PostgreSQL work and observe provider dispatch and the buffered answer before releasing it. Local concurrent admissions reserve atomically, tenant state stays isolated, and ordinary local quota exhaustion keeps its existing refusal. Multi-server tests establish approximate enforcement without pretending the configured total is a strict global ceiling. |
| **AC-024 — Async usage lifecycle** | REQ-030–031 | Real PostgreSQL persistence/refresh/retry proofs retain pending contributions, aggregate servers without overwriting usage, and do not double-apply committed updates. Cold/unhealthy/full states return the existing retryable 503 before dispatch and recover through background work. Cancellation/streams release local concurrency at execution end. Restart reloads committed usage; loss of unflushed process-memory usage is accepted and not labelled durable recovery. |
| **AC-025 — Prepared validation and Agent fixes** | REQ-032–033 | Warm repeated checks avoid repeated schema preparation; changed/bound/hook-replaced schemas validate correctly. Invalid answers, custom-data bounds, citations, repair and code checks retain behavior. Canonical Verification reports expose saved Gateway phase/wait evidence, and implemented Agent reductions have attributable correctness and performance evidence. |
| **AC-026 — Small owner-native implementation** | REQ-028–033 | Reuse the existing Gateway cache/worker/task and Skald/Vala execution owners. No Redis, new coordination service, crash-safe local ledger or strict global-quota machinery is introduced. Focused regression/integration evidence is required; TASK-006 needs no TDD cycle. No new benchmark runs without the existing explicit authorization. |
| **AC-001 — One recording workflow** | REQ-001–004; INV-001,003 | With benchmarking off, the production recorder remains installed and no benchmark observation accumulates. With it on, only the benchmark recorder is installed: counters increment, gauges update, and timing/diagnostic histograms retain their units and populations through benchmark-owned snapshots. Duplicate timing instrumentation does not double-count. Driver reconciliation, backlog and resource readings work without a Prometheus recorder, exporter or exposition parser. Cover all three engines and both benchmark levels through focused tests and consumer inspection; any live experiment requires the authorization above. |
| **AC-002 — Reliable intervals** | REQ-003,004,007; INV-004 | Focused tests preserve per-request residuals, phase classification, warmup separation, late-cohort counts, distribution bounds and invalid-evidence detection. Existing records remain readable. Missing evidence is explicit, and maximum declared workload coverage fits bounded recording capacity. |
| **AC-003 — Defensible c64 diagnosis** | REQ-005–007 | A report cites the existing records, applicable profile intervals and source mechanisms. It compares client and server p50s independently, distinguishes response latency from background work, and identifies where CPU profiles cannot explain waiting. Assertions of outbox saturation require outbox evidence; Scribe memory use alone is insufficient. |
| **AC-004 — Actionable diagnosis** | REQ-008; INV-001–003 | The TASK-002 research report ranks supported costs and proposed remedies, identifies the smallest falsifiable measurement for each unresolved material cause, and states what evidence would select or reject each remedy. It makes no unsupported root-cause or target-attainment claim. TASK-002 remains research-only; TASK-003 implements the four selected optimizations. |
| **AC-005 — Candidate disposition** | REQ-009; INV-001–004 | A compact change inventory identifies what is reused, reworked or removed from the unfinished candidate and which checks remain. The recording outcome has focused regression tests, formatting, lint and dependency-boundary evidence; no interrupted run is claimed as complete. |
| **AC-006 — Qualified recorder** | REQ-010; INV-001–004 | Independent-series progress and concurrent-snapshot tests preserve counts and complete timing groups. Valid, fully instrumented candidate evidence meets the task's declared recovery envelope against saved pre-task results and reports instrumentation costs and repetition spread. Recovery is a required intermediate milestone, not task completion. |
| **AC-007 — Reproducible terminal diagnostics** | REQ-003–004,011; INV-004 | Each repetition's declared phase distributions, units, populations and missing evidence survive record publication. Fresh/saved rendering agrees after raw artifacts are removed; supported historical records remain readable. |
| **AC-008 — Four optimizations delivered** | REQ-012; INV-001–004 | Each of the four paths has an implemented reduction in avoidable work, focused behavioral/allocation or ownership proof, and affected real-server regression evidence. The final unprofiled candidate uses the same corrected recorder/settings as the recovery milestone; report client/residual latency, phase evidence, CPU/work and throughput for every repetition. No optimization is deferred, and no response, redaction, audit, tenant, retry, byte-accounting or acknowledgement rule is weakened. Measured benefits and unresolved attribution are stated without claiming unsupported target attainment. |
| **AC-009 — SDK call reductions** | REQ-013; INV-001–004 | Focused request-backing/replay, timeout, binding, auth and real SDK Verify regressions prove less temporary work without altered deadlines, identities, errors, refresh, retries or public behavior. |
| **AC-010 — Native JSON/transport reductions** | REQ-015; INV-001–004 | Borrowing/ownership and compatibility tests establish the avoided request tree, consolidated Gateway-native response pass and single-chunk reuse through real consumers. Negative/edge and real-server cases preserve validation, canonical bounds, capture, provider errors and all fallback paths; standalone raw API behavior stays unchanged. |
| **AC-011 — Fixture equivalence** | REQ-014; INV-001,003–004 | Differential predicate tests preserve old driver/stub validation and routing, including escapes/duplicates. Borrowing and scoped-header proofs show reduced ownership/setup without changing workload, pacing or the completion boundary. Method controls disclose fixture changes. |
| **AC-012 — Current, reproducible attribution** | REQ-003,006–007,011,016; INV-004 | Fresh current-reference and production-candidate server heaps have declared populations and units. Unprofiled production and fixture comparisons retain distinct milestones and every repetition through the canonical renderer. Identical optimized server hashes isolate the final driver/stub comparison. Old allocation totals, missing samples, mean/median confusion, changed fixtures or sampled CPU are never used to invent current allocation, production gains or a hardware limit. |
| **AC-013 — Recorder parity** | REQ-017–018; INV-001,003–004 | Disabled clients produce no benchmark state; enabled startup installs one backend/registry before calls. Driver/SDK/stub observations retain precision, terminal outcomes, retry/cohort counts and bounded classification. A conflicting recorder, dropped series, missing cancelled call or hidden ordinary-mode initialization fails. |
| **AC-014 — Full path and distinct populations** | REQ-018; INV-001–004 | Focused/journey tests and saved reports identify driver versus server, SDK versus direct caller versus stub, provider headers/body and actual dispatch/handoff/hosted setup. Original boundaries/response ownership remain unchanged. No cross-process differencing, additive overlapping percentiles or exchange-as-external assumption occurs. |
| **AC-015 — Symmetric profiling** | REQ-019; INV-001,003–004 | CPU/heap/both modes target and identify both processes, preserve each allocator and existing accounting, and retain named stacks/units/binary identities. CPU-only has no heap observer; heap-only has no CPU sampler. Shared sampler and ordinary SDK dependency closure pass focused tests and dependency checks. |
| **AC-016 — Model-backed Verification coverage** | REQ-020; INV-001–004 | Local real-SDK diagnostics execute one model check with the original authority/deadline and valid citation/usage; upstream/result/evidence counts reconcile. Assertions retain old expectations/judgments and model cases remain separately identified diagnostics. |
| **AC-017 — Improvements implemented** | REQ-021; INV-001–004 | Two source-proven reductions ship with equivalence tests. Profile-informed candidates have implemented supported reductions or source/profile counterevidence, never an unexplained deferral. Trace fidelity, unused-facility validation, caller isolation and capture/durable ownership remain intact. |
| **AC-018 — Qualified, canonical results** | REQ-003,005–007,011,022; INV-004 | Reference→measurement-complete and measurement-complete→optimized comparisons distinguish observation cost from code gains. Both processes have valid case-scoped CPU/heap artifacts and per-repetition client/server/call-attempt evidence in regenerated canonical reports. No task completion from a profiling report alone or an unmeasured last fix. |
| **AC-019 — Useful compact results** | REQ-023; INV-001 | Direct, queued status/wait and typed reads preserve passed/failed/inconclusive/not-applicable judgments, reasons, actuals, citations, custom data and drift findings. Compact wire/schema/SDK results omit configuration, provenance, steps and input echoes. Reading a result makes no detail call or Prompt compilation; typed-data refusal never reruns judgment. |
| **AC-020 — Faithful descriptive details** | REQ-024–025; INV-001,003 | An inline Agent with no standalone Agent/Prompt Card retains its exact authored check, provenance, steps and evidence. Details and compact result agree on identity and judgments; repeated and historical retained reads do not compile or execute a Prompt. Existing storage facts/shapes remain intact. Invalid authored/model inputs are still refused at their existing execution boundaries. |
| **AC-021 — Availability and access are explicit** | REQ-026; INV-001,003 | Real-server journeys cover visible detail, unknown/not-yet-visible/missing detail, read denial and cross-tenant access. Missing visible detail uses the existing not-found code without fabricated evidence or lifecycle changes. A completed direct verdict remains useful when recording fails; no read authority, hidden IO or durability barrier is introduced. |
| **AC-022 — Migration and measured closure** | REQ-027,022; INV-004 | All first-class SDKs and HTTP/MCP expose compact results and explicit descriptive details with matching schemas/docs. Existing full-detail consumers migrate. Final canonical comparisons retain every repetition, both process profiles and response bytes, identify removed decode/compiler work, and state improvements/regressions/unresolved results separately. No old O result is relabelled as evidence for the new contract. |

## Existing owners and evidence

- [Benchmark workflow and measurement definitions](../../../benchmarks/README.md),
  the `bench:*` tasks in [mise.toml](../../../mise.toml), and
  `wyrd-testing::capacity::{Population, Window}` / `experiment::Record` own
  experiments, populations, resource windows, history and reports.
- `wyrd-bench::recorder::BenchmarkRecorder` already implements the standard
  `metrics::Recorder` interface for counters, gauges, peaks and HDR histograms.
  Its summary snapshots do not supply the full population/interval evidence
  required here. Reuse its instrumentation approach and the existing precise
  timing machinery in one narrow owner; do not import the test-tooling crate
  into production dependencies.
- The committed precise timing recorder was `wyrd_telemetry::timing`; the unfinished move is
  [wyrd-benchmark](../../../crates/shared/wyrd-benchmark/src/lib.rs).
  [Server metrics startup](../../../crates/wyrd/wyrd-server/src/app/metrics.rs)
  owns activation, monitoring installation and the loopback snapshot endpoint.
- [Gateway invocation](../../../crates/wyrd/wyrd-server/src/components/gateway/invocation.rs),
  its telemetry and capture modules, `GatewayEngine` and provider exchange timers,
  the shared runtime outbox and server Scribe sink are the existing attribution
  and execution owners. Extending their observations does not select a new
  capture execution policy.
- Baseline Gateway record:
  `benchmarks/history/gateway/overhead/20261108T234942Z-5508cf29f.json`.
  Candidate unprofiled record:
  `benchmarks/history/gateway/overhead/20261109T153318Z-b1cf95f2e.json`.
  Candidate profile record:
  `target/benchmarks/gateway/overhead/20261109T153854Z-b1cf95f2e.json`;
  raw profile and timing snapshots are under its `20261109T153854Z/` artifact
  directory. The baseline profile and candidate rerun named in the handoff are
  additional existing evidence. Local artifact availability and matching symbols
  must be checked before using derived profile conclusions.

## Task boundaries

The existing tasks are [sole benchmark recording](tasks/TASK-001-sole-benchmark-recorder.md)
and [Gateway diagnosis and proposal](tasks/TASK-002-gateway-latency-diagnosis.md).
They retain their original revision-2 scope. The maintainer explicitly added
[recorder recovery, standard reporting and four hot-path optimizations](tasks/TASK-003-recorder-overhead-and-standard-reporting.md)
as one required implementation task. Its ordering is recorder/reporting repair,
measured baseline recovery, all four optimizations, and final correctness and
performance verification. TASK-003 retains its revision-3 scope.
[TASK-004](tasks/TASK-004-client-and-json-hot-path.md) encompasses all remaining
client, fixture and native JSON/transport reductions plus their current allocation
and staged performance proof. Do not split either task's optimization work into
future tasks or insert another general diagnosis gate. New experiments retain
the explicit execution-authorization boundary above.

[TASK-005](tasks/TASK-005-full-path-benchmark-parity-and-optimization.md) owns
client/server metric and profiling parity, full controlled-path diagnostics,
model-backed Verification, the supported small reductions and final qualification.
Earlier task scopes/evidence remain intact; TASK-005 completes this additional
work rather than reopening their finished implementation.
Its appended revision-6 block also owns compact Verification responses, descriptive
details, complete retained evidence, cross-surface migration and final measurement.
Earlier TASK-005 execution evidence is historical proof of its revision-5 work,
not completion of this extension.

## Open material decisions

Revision 7 records the maintainer's 2026-11-11 approval and subsequent explicit
choices: approximate enforcement is sufficient; unflushed usage may be lost on
crash; cold/unhealthy/full cache state refuses until background recovery; and
TASK-006 does not require TDD. No material decision remains open for this scope.
Batch sizes, refresh cadence, bounded capacities and private preparation sharing
remain implementation choices. Do not add strict global coordination, local
durable storage or another service to solve a guarantee the user did not request.

For Gateway usage/admission, revision 7 supersedes the gateway-capacity revision-4
requirement that covered buffered calls settle before answering. That earlier
packet remains historical evidence for its own scope. Other security, capture
and evidence constraints remain unchanged. The owning Gateway quota section
of `architecture/wyrd-design.md` records the revised contract.

Revision 2 was explicitly approved on 2026-11-09. Revision 3 records the
maintainer's explicit scope correction on the same date: all four identified
optimizations belong to TASK-003 after recorder recovery, with no deferral.
No new capture policy, security semantics or retry protocol is selected.
Revision 4 records the 2026-11-10 instruction to plan all client, fixture and
native JSON/transport work as one decision-complete TASK-004. It changes no
security, durable protocol, public SDK/API or latency judgment. Additional
benchmark execution remains separately controlled as described above.

Revision 5 records the explicit client/server parity and full-path profiling
instruction and the 2026-11-10 request to create TASK-005. Collector mode semantics,
the narrow shared heap feature and the local model diagnostic close the material
measurement decisions for that scope; product/security/durable behavior is unchanged.

Revision 6 was explicitly approved by the maintainer on 2026-11-10. The route,
field split, missing-detail response, unchanged read authority and storage
compatibility above close the material decisions. The owning Verification result and
route sections of `architecture/wyrd-design.md` and the three-table assembly prose
in `architecture/bifrost-design.md` must be updated in the same implementation
change; their current full-default-result description is explicitly superseded
when that cohesive update lands.

## Authority and revision history

- Revision 7 approved scope, 2026-11-11: explicit instruction to approve the
  TASK-006 spec revision; memory-owned per-server usage/reservations,
  asynchronous PostgreSQL writes/refresh, approximate quotas including budgets,
  accepted loss of unflushed usage after crash, and retryable refusal until
  background recovery for missing/unhealthy/full cache state. Follow existing
  owners and the simple cache-first/post-response-accounting pattern; no strict
  cluster quota guarantee or new durable service. Remove repeated Agent schema
  preparation and implement supported latency fixes. Explicitly waive TDD for
  TASK-006 while retaining regression/integration/performance proof. Approval
  authorizes planning; implementation, experiments and Git actions remain separate.
  An independent read-only comprehension pass over revision 7 and TASK-006 found
  no material explanation gaps or contradictions after these choices were recorded.

- Revision 6 approved, 2026-11-10: compact default Verification results, explicit
  descriptive execution details, no compile-on-read, preserved complete records
  and asynchronous visibility, unchanged read authority, coordinated SDK/schema
  migration and TASK-005 follow-on proof. Explicit approval of the written
  contract authorizes planning; implementation and experiments remain separate.

- Revision 5 approved scope, 2026-11-10: explicit maintainer instruction to use
  the same benchmark collection and profiling for client/server, profile the full
  Gateway/Verification path, and create TASK-005 for that work and supported
  improvements. Records this directed scope and bounded measurement decisions;
  planning itself authorizes no implementation, builds or experiments.

- Revision 4 approved scope, 2026-11-10: explicit maintainer request for one
  decision-complete TASK-004 encompassing the remaining SDK, fixture, native
  request/response and buffer work, fresh allocation evidence and separate
  production/fixture performance attribution. Records that directed scope;
  planning itself runs no implementation, build or experiment.

- Revision 3 approved scope, 2026-11-09: explicit maintainer instruction that the
  four identified hot-path optimizations are part of TASK-003 and must not be
  deferred. Adds recorder qualification, canonical diagnostic reporting and
  implementation/verification obligations for those four paths. This records
  the directed scope, not permission to execute benchmarks or implementation
  during planning. All existing product invariants remain in force.

- [Wyrd workflow](../../spec-driven-development.md) and
  [bounded implementation execution](../../implementation-execution.md).
- [Principal capture contract](../../../architecture/wyrd-design.md#principal-model),
  [Bifrost durable ingest](../../../architecture/bifrost-design.md), and
  [operational indicators and overload](../../../architecture/operations/reliability-and-recovery.md).
- [Testing](../../../TESTING.md) and repository `AGENTS.md` apply.
- Revision 1, draft, 2026-11-09: captures the agreed single-recorder direction and
  restores diagnosis/proposal scope. It neither approves nor replaces the broader
  draft [Gateway latency and capture spec](../gateway-latency-capture/spec.md),
  and does not reopen or complete resource-scaling TASK-005.

- Revision 2, draft, 2026-11-09: makes the benchmark backend the sole process
  recorder for timings, counters, gauges and diagnostic histograms. Removes
  revision 1's proposed retained Prometheus counter/gauge backend and explicitly
  requires benchmark-owned snapshots, exclusive startup selection and protection
  against duplicate timing observations. No implementation change accompanies
  this revision.

- Revision 2 approved, 2026-11-09: explicit maintainer instruction to approve
  the specification and create the plan. Approval changes no requirement and
  does not authorize execution. For benchmark collection, this revision replaces
  older Gateway-capacity packet mechanics that read Prometheus exposition;
  production telemetry contracts remain unchanged.
