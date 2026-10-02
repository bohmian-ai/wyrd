---
id: TASK-008
kind: implementation
status: proposed
spec: SPEC-verified-change-contract
spec_revision: 45
requirements: [REQ-089, REQ-101, REQ-114, REQ-146, REQ-151, REQ-152, INV-015, AC-017, AC-020, AC-021, AC-022, AC-023, AC-024, AC-030, AC-032, AC-033]
depends_on: [TASK-005, TASK-006, TASK-009, TASK-010, TASK-012]
---

## Outcome and Value

Prove the complete Drift and Eval change through real clients, server,
Postgres, Bifrost, results, and Operator delivery. A short opt-in production
benchmark drives the same public client journey at a stated rate and reports
what the client experienced, what the server accepted, how many verifications
ran, and where time accumulated. Production runtime metrics and correlated
traces make queueing, execution, publication, settlement, and delivery
diagnosable.

This task also closes the existing Rust, Python, TypeScript, HTTP, MCP,
multi-server, restart, authorization, tenant-isolation, resource-ceiling, and
architecture proof matrix. It adds no new Verifier kind, durable workflow, or
public contract.

## Owners, Scope, Consumers, and Prohibited Changes

`wyrd-server` and its existing SQL/runtime owners emit verification telemetry
at real transitions. Reuse the server's Prometheus and tracing setup; do not
create another telemetry service, process-local work ledger, or exporter.
`wyrd-testing` owns the benchmark and existing journey registration. Reuse the
Bifrost query-capacity benchmark's release-server process, repository-managed
Postgres, bounded resource envelope, `/metrics` scraping, and report patterns.
The ordinary in-process `WyrdTestServer` disables metrics and is not the
benchmark server. The public Python SDK drives the scoped Run/OTLP journey
after TASK-009; public Rust, Python, TypeScript, HTTP, and MCP journeys retain
their separate contract proof.

The benchmark is opt-in and outside `mise run gate`; cold release compilation
is not counted as traffic duration. Keep setup and registration visible but
separate from steady-state measurements. Do not register a Service or table on
every observation, directly insert verifier jobs, call an engine in-process,
substitute manual Drift for scheduled Drift, or claim an observation write is a
completed verification. Do not add code-review, CI, or synchronous Verifier
executors that this change does not ship. Do not infer automatic log or metric
correlation from TASK-009's Python trace correlation.

Do not repair a failing journey by weakening assertions, mocking away a real
server/dependency, adding sleeps, changing approved behavior, or building a
second harness. A local deterministic Eval configuration and local Operator
endpoint may stand in for external providers; the real server and engines
must still execute. No production credential is required.

## Approach

1. Audit every REQ/INV/AC against landed proof and close only missing
   cross-owner seams in existing journey homes.
2. Instrument the real verification runtime at its queue, execution, result,
   and Operator boundaries. Reuse existing metrics and SQL spans; add only
   measurements that answer a distinct latency or backlog question.
3. Extend the production-process benchmark setup to register bound Services
   and a custom Bifrost table through public clients, then drive bounded Run,
   observation, custom-data, and OTLP traffic through public endpoints.
4. Capture client timings, durable verification outcomes, production metric
   scrapes, correlated traces, and the server resource envelope in one report.
   Reconcile offered, accepted, activated, terminal, and queued work.
5. Complete the integrated cross-language and multi-server proof matrix,
   regenerate affected public artifacts from source, and remove stale
   architecture/build references to retired surfaces.
6. Run focused Red-Green checks during iteration, then the repository gate
   and opt-in benchmark as distinct final evidence.

## Ordered Implementation Scenarios

### Scenario 1 — Runtime telemetry locates verification delay and failure

**Behavior.** Production `/metrics` reports the existing queue depth, active
permits, attempts, failures, schedule ticks, Eval post-ACK enqueue failures,
Operator attempts, and claim-to-settlement duration. Add queue-wait and
trigger-to-terminal-state duration, with bounded implementation, trigger,
and outcome dimensions only where they answer distinct questions. Record
non-overlapping load, engine, publication, and settlement phase durations
where the runtime owns those boundaries; retain the existing total attempt
duration. Waiting and active work remain separate. Counters record the actual
owner transition;
completed results are counted only after durable settlement. Retry, lease
recovery, cancellation, and failed settlement remain visible without being
counted as completed work. Measurements use PostgreSQL-owned coordination
times or process-local `Instant` as appropriate under REQ-152, never a direct
process-wall-clock comparison to a database deadline.

One correlated server trace follows an attempt through claim, evidence read,
Drift/Eval execution, result publication, and settlement; retries and
Operator delivery can be related by scrubbed run/result identity. A terminal
span records outcome or error. SQL spans remain children where applicable.
Trace payloads exclude evidence contents, credentials, tokens, and sensitive
logs. Tenant, Card, Run, request, verifier, and result identifiers are never
metric labels. Routine successful transitions do not create duplicate INFO
logs.

**RED.** Add focused production-path telemetry assertions around real queued,
completed, retried, cancelled, and failed runs. Before implementation, the
existing claim-to-settlement histogram cannot explain queue wait or full
trigger-to-result latency, and no continuous server run span covers the
attempt. Assert emitted values and span correlation from actual transitions,
not metric registration or synthetic samples.

**GREEN.** Add missing bounded measurements and spans in their existing
owners, preserving runtime limits, best-effort Eval enqueue, durable state,
and public errors. Reuse existing metric families rather than duplicating
attempt or activity counts.

**REFACTOR.** Remove redundant success logs or overlapping metric series
only when production-path assertions still explain rates, backlog, failures,
and phase timing.

### Scenario 2 — Public clients generate production-shaped verification work

**Behavior.** A real public client registers Services with exact Drift and
Eval bindings and a caller-owned Bifrost table once during setup. During the
timed workload it opens Runs, writes Drift and Eval observations and custom
table rows, emits authenticated OTLP traces/logs/metrics, flushes/shuts down
the client, and reads status/results through public paths. Python Run scopes
exercise TASK-009's trace correlation. Eval activation follows acknowledged
observations; scheduled Drift runs at its configured cadence. A separately
labelled manual Drift slice may exercise its public run API. Failed results
reach the integrated local Operator endpoint. No direct SQL seeding or
engine-only invocation substitutes for the timed client-to-server path.

**RED.** Extend an existing real-client journey with registered bindings,
custom data, OTLP signals, result reads, and failed-result delivery. It fails
because the currently separate observation and verification journeys do not
prove this complete path together. Add focused benchmark smoke coverage that
fails when a public write is mistaken for an activation or result.

**GREEN.** Reuse the existing SDK/Run journey and Bifrost benchmark's
release-server startup, local Postgres/storage, resource limits, and scrape
support. Keep deterministic Eval and local delivery fixtures bounded and
credential-free while the real Drift/Eval runtime executes.

**REFACTOR.** Share existing server startup and telemetry parsing rather
than copying them; keep benchmark workload distinct from correctness journeys.

### Scenario 3 — A short benchmark reports capacity without hiding backlog

**Behavior.** An opt-in `mise run bench:verification:journey` runs a fixed,
documented default profile: 20 seconds warmup, 120 seconds steady traffic,
20 seconds burst, and 40 seconds without new arrivals to observe drain.
The traffic phase is about 3 minutes 20 seconds; target a total run of a few
minutes with prebuilt binaries and ready local infrastructure. Set up eight
tenants with bound Services. Offer 50, 100, and 150 client Run iterations per
second in warmup, steady, and burst phases respectively, independent of
response time. Each iteration emits one Eval observation, one Drift
observation, one custom row, and bounded OTLP trace/log/metric signals, using
declared payload sizes and batching. A small deterministic failure fraction
exercises the local Operator endpoint. Scheduled Drift remains an additional
measured stream at its configured cadence. This profile offers approximately
100 Eval activation opportunities per second in steady state, not 100
guaranteed activations or completed verifications. Report planned versus
actually submitted traffic and generator saturation. Report the mix and
resource envelope as assumptions, not as measured production traffic. Do not
claim 100 completed verifications per second unless results show it.

The report distinguishes setup time, client write/flush latency,
acknowledged evidence, Eval activations, scheduled and manual Drift runs,
terminal results, failed/queued/retrying work, Operator delivery, queue depth,
queue-wait distribution, and server CPU/memory. For each meaningful phase,
report throughput and p50/p95/p99 latency with the measurement boundary stated.
Take production `/metrics` snapshots at phase boundaries and capture sampled
server traces so a short scrape interval cannot smooth away the burst.
Reconcile client and server counts; a best-effort Eval enqueue failure is
reported as explicit loss, not silently equated with a run. Silent loss,
wrong-tenant results, missing expected evidence, or failed report
reconciliation fail the benchmark. Capacity below the offered target is
labelled below target and reported with backlog and latency; it is not
disguised as a green 100/s claim. The short run makes no long-term soak,
autoscaling, or 10–20 minute code-review completion claim.

**RED.** Add a bounded report/driver check in the existing benchmark owner
that injects a known offered/accepted/completed/backlogged mismatch and proves
the report refuses to call it success. A production-process smoke run
initially lacks the verification journey, phase-separated timing, and
metric/result reconciliation.

**GREEN.** Add the smallest opt-in mise command and benchmark driver using
existing `wyrd-testing` process and telemetry facilities. Emit a concise
human summary and machine-readable results, including offered rate, achieved
verification rate, latency, resource envelope, and accounting counts.

**REFACTOR.** Keep one workload profile and one report path; add tunable
inputs only for values needed on a different deployment envelope or to
reproduce a measured bottleneck.

## Proof Strategy for Existing Contracts

The remaining integrated matrix verifies already-required behavior and does
not manufacture a RED for absent test registration. Each added journey must
fail first on the expected missing seam or registration, then pass without
inventing a new product contract. The matrix must include:

- Rust/Python/TypeScript registered Service journeys containing PSI, SPC,
  Custom, deterministic Eval, and local LLM-judge Eval bindings; each crosses
  exact-principal authentication, locked run API, queue/IPC, Gate/Scribe,
  runtime, Bifrost result query, status, and one failed-result Operator.
- Python framework-created spans inside `with state.run(card="...")` carrying
  the exact run ID and CardRef through OTLP to server-resolved Bifrost Card UID,
  including persisted joins to caller-owned custom rows by `run_id` and Eval
  rows by trace/span identity, nested Card scopes, asyncio isolation,
  private-provider install, and fail-open absence or failure of optional
  OpenTelemetry integration.
- HTTP/MCP manual binding and direct Drift runs with requester identity,
  nullable direct owner/binding, idempotency, unauthorized and cross-tenant
  refusals, and existing Bifrost query for result/detail rows.
- Multi-server result writing where the worker owns no Scribe, including the
  complete internal tenant SYSTEM identity/table matrix, public principal and
  credential-path refusals, single existing-issuer/JWT reuse, one Gate audit
  decision, and one raw observation feeding two independently filterable
  bindings.
- Daily partitions and physical Bloom evidence across two time partitions,
  Oracle partition/row-group pruning, exact Eval record-day lookup, and one
  result event time across ACKs straddling UTC midnight.
- Duplicate record/batch/cron handling, expired leases, retry exhaustion,
  fail-open Eval enqueue, partial result visibility without settlement,
  zero-detail summary-only publication, restart recovery, and no stale old
  table/route/crate path.
- PostgreSQL-owned activity, schedule, claim, lease, retry, and dispatch
  deadlines, with no process wall-clock predicate, skew workaround, injectable
  coordination clock, or permanent source checker.
- Permission allow/deny audit at every public/Gate boundary, no audit for
  internal mechanics, per-tenant/global saturation fairness, Operator timing
  ceilings, shutdown drain/reclaim, supervisor health, and required telemetry.
- Operator connection CRUD/rotation/redaction and local Slack/PagerDuty/HTTP
  protocol fixtures, including typed path IDs, secret-free CLI argv/debug,
  served OpenAPI, and runtime MCP catalogs. Credentialed live smokes remain
  release-gated evidence.
- Exact Card-bound API-key and workload-`jwt-bearer` exchanges activate owners;
  delegation, human refresh, Card-free automation, SYSTEM issuance, cached
  bearer requests, and idle expiry do not. Principal suspension blocks new
  work immediately while ordinary authorization retains the current
  five-minute permission-snapshot semantics.

## Acceptance Criteria

- Every spec obligation maps to a passing strongest applicable proof or an
  explicitly inherited static contract; no acceptance criterion relies only on
  a lower tier when a journey is required.
- One public-client benchmark exercises registration, custom-table creation,
  Run/observation/custom-data/OTLP ingestion, actual Drift/Eval execution,
  result query, and failed-result Operator delivery against a release server.
- The report separates offered client events, acknowledged records, activated
  runs, settled results, and backlog. It names rates, latency boundaries,
  workload mix, tenant count, and hardware envelope; it never treats a write
  ACK or `202 Accepted` as a verification result.
- Production metrics and traces explain queue wait, attempt execution,
  publication, settlement, failure, retries, and delivery without unbounded
  metric labels, sensitive payloads, duplicate success noise, or a second
  telemetry system. A captured trace and nonzero production scrape reconcile
  with durable/client facts.
- The benchmark's default measured traffic fits in a few minutes, is opt-in,
  and reports achieved capacity honestly rather than imposing an unsupported
  100/s success threshold or claiming long-running-workflow capacity.
- All user-facing surfaces use the same server-owned durable behavior and
  stable errors.
- The current tree contains no stale authoritative alternative, generated
  drift, retired route/table/crate/check reference, or unregistered journey.
- No material decision is made in this task; discoveries requiring one return
  to `$wyrd-spec`.

## Expected Write Set and Consumer Closure

Likely owners: verification runtime and SQL queue telemetry in `wyrd-server`
and `wyrd-sql`; existing server metrics registration; `wyrd-testing` journey,
release-process benchmark, telemetry capture, and report; one opt-in task in
`mise.toml`; language-specific journey homes; generated artifacts and
permanent architecture/public docs only when an approved contract requires
them. These paths are guidance, not an implementation allowlist. Production
fixes outside instrumentation remain bounded corrections to an existing
in-scope seam and must be recorded.

## Verification and Evidence

During Red-Green iteration, run each added specifically named test through
its exact repository-native focused command, including the Postgres setup
wrapper for DB-backed tests. Record test names and commands once the owning
test targets are selected; do not use a selector that can pass with zero
tests. Capture benchmark stdout/report, production `/metrics` snapshots,
sampled traces, server logs on failure, and durable result counts from the
same run. The opt-in benchmark is not part of `mise run gate` in the current
`mise.toml`.

```bash
mise run gate
mise run bench:verification:journey
git diff --check
```

## Material Stop Conditions

Stop for any need to change public/persisted contracts, security or tenancy
semantics, the accepted best-effort Eval enqueue ceiling, resource limits,
coordination-clock ownership, or test-tier requirement. If the benchmark
cannot complete its default measured workload in a few minutes, report the
actual timing and adjust the workload/profile within approved scope; do not
hide startup time or replace production paths with mocks.

## Authority Links

- `changes/active/verified-change-contract/spec.md`
- `changes/active/verified-change-contract/integration/TASK-012-replay-operator-connections-and-delivery.md`
- `changes/active/bifrost-scribe-live-reads/tasks/TASK-005-simplify-bifrost-telemetry.md` (proposed design precedent, not completed behavior)
- all documents under `changes/active/verified-change-contract/architecture/`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/wyrd-security-posture.md`
- `architecture/references/domain/telemetry-observations.md`
- `architecture/references/languages/testing-workflows.md`
- `AGENTS.md`
