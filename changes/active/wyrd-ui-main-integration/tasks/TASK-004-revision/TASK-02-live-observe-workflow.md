---
id: TASK-004-REV-02
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-main-integration
spec_revision: 18
requirements: [REQ-007, REQ-015, REQ-016, REQ-017, REQ-020, REQ-021, REQ-022, REQ-023, REQ-028, REQ-030, REQ-034, REQ-038, REQ-039, REQ-040, AC-008, AC-010, AC-011, AC-013, AC-016, AC-017, AC-018, AC-021, AC-022, AC-023, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007]
depends_on: [TASK-003, TASK-027, TASK-004-REV-01]
parent_task: TASK-004
remediates: []
---

# TASK-02 — Working live shell, traces, logs, metrics and canonical dev batch

## Outcome and Value

`mise run dev:full` starts Wyrd, its real authenticated UI, the canonical
`bohmian-ai/wyrd-example-py` application and Task 01's local fixture service.
It runs a bounded 3,000-request batch, then leaves everything available for
investigation. A developer signs in and sees actual retained traces, spans,
logs and metrics, opens their details, and observes new data without a reset.

This is the complete delivery task that closes TASK-004, not a gallery-only
exercise or another partial stage. It absorbs the full shell and page outcomes
of TASK-012, TASK-020, TASK-021 and TASK-022, plus all outstanding TASK-004/R1
shared-component obligations. Their older task documents are historical
scope/evidence references, not parallel execution assignments.

## What Changes and What Must Stay True

### Standard development workflow

- Extend `mise.toml::dev:full` and existing process/database cleanup owners.
  Start the real production Rust server, local Postgres, Task 01 MockServer,
  ordinary SvelteKit development UI, and canonical app. Build/run dependencies
  in readiness order; keep Vite reload independent from backend/batch startup.
- Default batch size is 3,000 and concurrency is 8. Support the same workflow
  with a smaller bounded workload via `mise run dev:full -- --requests 20
  --concurrency 2`. Add `mise run dev:batch -- --requests 20 --concurrency 2`
  to append another batch to the running environment without restarting it.
  No benchmark framework, load generator or synthetic host-load test is needed.
- Preserve the existing ephemeral backend lifecycle and disclose its startup
  reset/credential replacement. Retained evidence lasts while that environment
  runs; no restart persistence or new dataset-reset/schema-repair command is
  promised. A failed batch must leave healthy UI/server/app processes available
  for diagnosis. Ctrl-C cleans up owned children and drains app telemetry.
- Server/UI readiness and canonical setup/batch status are distinct. Report
  usable URLs and normal browser login, not a fake local session. Startup does
  not require paid provider credentials: normal dev OpenAI credentials point
  to Task 01's configured endpoint for application and queued judge calls.
- MockServer is optional tooling outside `dev:full`; preserve existing
  real-provider/`dev:both` behavior. Add no SDK scope, header, provider type,
  billing rule, evidence schema, verification propagation or scheduler change.

### Canonical application and reproducible SDK/source

Use canonical commit `03428e1a4a35c8f23f71a95cae08e956ca317c9b`, not the
different in-repo support-desk example. The dev launcher obtains a clean pinned
checkout at `target/dev/wyrd-example-py`. Deliver canonical application/driver
changes as a reviewed patch at `scripts/dev/wyrd-example-py.patch`; pin the base
once in the dev owner and record base, patch digest and effective source identity.
These durable inputs must survive later removal of the active change packet.
Do not overwrite a dirty checkout or silently follow mutable upstream main.
Local patch delivery completes this task; upstream commit/push is separate.

Build the host-compatible production Python wheel from the current Wyrd
checkout, place it in the canonical app's exclusive `vendor` source, and verify
the imported artifact origin/digest. Matching the version alone is insufficient;
neither a cached Linux wheel nor the testing-enabled development wheel may win.
The canonical source/dependency lock changes belong in its reviewable patch.

Keep the existing `AsyncOpenAI` construction and ordinary Agent/Prompt model
addresses. `Desk.handle` remains the sole application Run owner. Add ordinary
ASGI OpenTelemetry instrumentation without a second Run-creating middleware;
declare `opentelemetry-instrumentation-asgi`, already used in SDK development
tests, as a canonical application dependency. HTTP roots may lack Run/Card
attribution while their child spans retain shared trace context. Emit actual
application logs and metric counter/gauge/histogram observations through the
SDK-installed providers. Do not install another exporter pipeline, fabricate
Bifrost rows or add a development flush endpoint. Normal exporters drain while
running; existing graceful lifespan shutdown invokes public SDK shutdown.

The canonical `wyrd-example-py` patch explicitly includes the small revisions
needed to batch-export Python logs, metrics and traces. Connect application
Python logging to the SDK-installed log provider, emit the metric observations,
and use the existing SDK/OTel buffering and export lifecycle for all three
signals. Confirm export occurs while the app remains running and pending
observations drain on graceful shutdown. Existing app tracing alone does not
establish log or metric export; no second exporter stack is required.

### Finite batch, complete evidence

The canonical application owns the reusable batch executor. Workload requests
are actual application HTTP calls; setup/readiness/readback requests are reported
separately and never counted as workload. Use the existing three order IDs.

| Default case | Count | Expected observation |
|---|---:|---|
| Shipping `POST /ask` | 2,400 | HTTP 200, real tool exchange, immediate verdict passed, queued execution returned. |
| Refund `POST /ask` | 300 | HTTP 200, real tool exchange, immediate verdict failed, queued execution still returned. |
| HTTP-only `GET /openapi.json` | 150 | HTTP 200 and actual HTTP telemetry without a model call or Run. |
| Unknown-order `POST /ask` with `Z-9` | 150 | HTTP 404 before Desk/Run/model execution, retained HTTP error evidence. |

For smaller counts, allocate cases deterministically in those proportions,
rounding remaining slots into shipping; the dispatched total must equal the
requested count. No driver retries enlarge that denominator. Record actual
request/standard trace context, returned `request_id` (the Run ID), immediate
verdict, and `submit.execution_id` where present. Do not create grouping metadata.

Wait through the public queued-execution API with bounded deadlines and check
retained observations through public Bifrost reads after normal export. The
full default flow expects 2,700 Runs, 2,700 immediate evaluations, 2,700 queued
executions, 5,400 app model exchanges and 2,700 judge exchanges in the no-retry
case. Compare logical flow counts with actual captured attempts and disclose
any retries rather than conflating them. Task 01's uniform queued verdict is
`passed` with valid `/price` evidence; that is scripted fixture behavior, not a
claim about judge accuracy. Immediate refund failures remain expected failures.

Report HTTP results, Runs, direct/queued outcomes, gateway attempts and retained
spans/logs/metrics separately. Correlate request evidence by existing trace/Run
IDs; don't equate the number of spans/metric points with request count. Missing
telemetry, nonterminal executions, malformed verdicts, unexpected errors or
timeouts fail the batch. Empty UI data cannot be called a successful seed.

### Real shell and all three Observe tabs

Use one authenticated host at `/t/[tenantKey]/observe` with shareable signal
state `signal=traces|logs|metrics`; Traces also supports `mode=traces|spans`.
Default to traces. Preserve existing login/logout, tenant switching/settings,
theme and approved shell layout. Disabled future destinations must be explicit.
Use the existing session-bound `/ui/load` BFF, portable shared loaders and
Node executor; no fixture fallback or page-owned SQL/aggregation pipeline.

Range, applied predicate, mode, grouping, selected exact record and return state
must survive navigation/reload. Use one resolved range/effective predicate for
charts, facets and lists. Superseded/cancelled loads cannot overwrite newer
results. Preserve truthful loading/empty/denied/error/partial states, exact
boundaries, missing values, payload denial and tenant/table authorization.

| Surface | Complete outcome |
|---|---|
| Shell/shared controls | Approved navigation/layout, TimePicker presets/custom range/timezone, Live toggle, WHERE/facets, grouping, chart actions, responsive keyboard/touch behavior. |
| Traces and Spans | Three overview charts, trace roots versus flat spans, visible-row counts, paused/held rows and counts, new-row notice/resume, exact trace drawer and span selection, waterfall axis/collapse/expand/errors-only, recorded messages/attributes/exceptions/raw details. Exact authorized trace evidence includes untagged/cross-workload/out-of-list-range spans. |
| Logs | Timeline/severity facets, actual `severity_number`/`severity_text` and body/attributes, wrap lines, bounded paging/page size/columns, selected detail and sensitive denial, attribute Filter/Exclude into WHERE, Open trace with preserved Logs return state. Record names never determine severity. |
| Metrics | Selector, recorded-dimension grouping, chart/legend and selectable series Latest/Max/Mean synchronized with line visibility. Raw points alone do not complete the page. Keep kind/unit/temporality/monotonicity and incompatible series separate. Preserve gauge/sum integer/double, histogram structures, recorded summary quantiles/exemplars. No fabricated percentiles/rates/zeroes or invented metric-to-trace link. Unsupported summaries/plots are explicitly unavailable. |
| Chart query inspection | `/t/[tenantKey]/query` retains exact executed SQL and typed binds; authorized bounded rerun/cancel/terminal/error/refusal. Reuse query owners; full Query catalog/History/chart authoring remains TASK-019. |

Preserve precision. Existing bounded JSON collection parses numeric values as
JavaScript numbers and cannot represent every int64 exactly. Extend the shared
Metrics planner/projection to transport exact display values as decimal text
through existing SQL/results, retaining recorded metric type separately; use
lossless server-side projection for nested counts where needed. Verify that
projection against real SQL. Numeric chart geometry must never silently replace
exact values: expose approximation or an unavailable plot when necessary.
Do not claim generic `plot.ts` numeric coercion, duplicate-point summation or
series folding is a semantically correct metric aggregation. This authorizes
no new JSON wire encoding or parallel Arrow transport.

### Safe predicates and inherited component acceptance

Close existing REQ-040 in the current Rust query/parser owner: an optional
single predicate over the selected signal table's described columns, 8 KiB
UTF-8 maximum; permitted scalar comparisons, boolean combinations, NULL,
IN/BETWEEN/LIKE/ILIKE only. No comments, statements, subqueries, relation changes
or caller parameters. Empty input means no extra predicate. The existing
planned read `POST /v1/query/predicate` / MCP `bifrost.validate_predicate` takes
`namespace`, `name`, `expression`, returns normalized `expression`, and checks
visibility/permission before schema disclosure. Extend `wyrd-client::Bifrost`,
all three SDKs and existing executors; no second SQL grammar or query service.
Invalid draft input remains visibly unapplied with the last successful data;
validation failure issues no dependent reads. Facets stay bound and combine
with parenthesized predicate/range templates. All execution still authorizes.

Carry all R1 obligations into this task: WyrdView/board-labelled component
states; faithful facets/list live notice and truthful extent/footer; waterfall
guidance/counts; recorded role blocks/tab counts/exception/attribute/raw detail;
generic-name and authored-alias semantics; typed group identities; Logs controls;
Metrics summaries/selection; StatStrip inventory and truthful board presentation.
Keep the individually accepted/proven behavior and repair only remaining gaps.
StatStrip is a shared summary with separate invocation/usage consumers; use
recorded fields and already approved invocation semantics, not a new generic
statistic inferred from names. Record the chosen existing board region per
consumer without changing its published inputs to resolve a visual mismatch.
The early-live exception permits this real-data work before unrelated family
acceptance, but closing TASK-004 still requires each owned component's evidence
and individual human acceptance. Archived implementation claims are not PASS.

## Approach

1. Revalidate shared branch work and Task 01 integration; retain its components,
   current data/auth owners and all inherited scope before changing anything.
2. Deliver canonical patch/current production wheel, ordinary bootstrap and
   bounded batch through existing lifecycle owners.
3. Close predicate consumers and missing shared Logs/Metrics semantics once at
   their existing owners, preserving portable Node/MCP parity.
4. Assemble the real shell, Observe modes, drawer/navigation and query inspector.
5. Complete deterministic real-browser journeys, replay the full dev lifecycle,
   record source/data evidence and present individual visual checkpoints.

## Ordered Implementation Scenarios

### Scenario 1 — One command reaches usable development readiness

**Behavior.** Startup selects the pinned canonical source/current production
wheel, initializes normal tenant/Cards/credentials and Task 01 fixtures, then
starts app/UI/batch once. Source/dependency/readiness/port failures are visible;
healthy services remain inspectable and owned children shut down correctly.

**RED.** Extend the planned V1 lifecycle coverage with a small workload. Confirm
the absent canonical startup/current-wheel/readiness behavior fails, not an
unprovisioned build prerequisite. Static source pin/config changes need static
proof; changed executable startup/failure decisions need runtime assertions.

**GREEN.** Extend `dev:full` and current process/bootstrap owners, without an
alternate environment controller, mutable source fallback or fake login.

**REFACTOR.** Consolidate duplicated task-body orchestration into its existing
owner as needed, retain setup/error/cleanup assertions, and avoid new test modes.

### Scenario 2 — Actual canonical workload becomes retained evidence

**Behavior.** The batch performs exactly its requested calls with bounded
concurrency, real tools/direct and queued verification, expected failures and
normal telemetry. Terminal waits/readback determine success. A failed/missing
observation makes the report fail while leaving the environment inspectable.

**RED.** Add planned V2 in the canonical patch and show missing case accounting,
terminal waits or retained readback on a small run. Explicitly assert Python
logs, metrics and traces are exported in batches and become queryable while
the app runs, with pending observations drained on graceful shutdown.
V1's integrated coverage
must reach the real app/server/MockServer, not the old conftest provider double.

**GREEN.** Implement the canonical executor/instrumentation and reuse public
execution/Bifrost reads; retain one Run owner. Rerun Scenario 1 and V2.

**REFACTOR.** Keep case/report logic in the canonical app, reuse normal SDK
providers/drain, and remove duplicate seed/flush/retry mechanisms.

### Scenario 3 — Safe signal predicates retain host parity

**Behavior.** Valid predicates consistently constrain the selected signal's
charts/facets/list. Invalid, injected, unauthorized and out-of-schema input
fails before dependent reads; applied state, typed facet binds and exact trace
navigation remain truthful in Node and MCP hosts.

**RED.** V3 adds predicate grammar/shape/adversarial cases and exact dependent
read assertions. Add V10–V13's planned focused validation journeys to the
confirmed Rust/Python/TypeScript/MCP query owners. Confirm their actual selectors
after adding them, before RED; absent or skipped tests cannot supply proof.

**GREEN.** Extend the existing parser, query contract, client projections and
portable loaders, then rerun V3 and affected owner regressions. Do not accept
full-query validation as proof of a safe expression fragment.

**REFACTOR.** Reuse one validator/query owner, fixed bounded query templates and
existing terminal/error handling; remove duplicated parsing/validation.

### Scenario 4 — Authenticated Traces and Spans investigation

**Behavior.** A real browser signs in, opens actual traces/spans, changes time/
SQL/facets, holds/resumes live data, opens exact evidence and span details,
inspects query SQL/binds and returns with state preserved. Empty/denied/error,
foreign-tenant, stale/cancelled and untagged generic HTTP cases stay truthful.

**RED.** Add planned V4, extending V3 with necessary host interaction assertions.
Show the missing deployed route/actions in an actual browser; jsdom or imported
handlers cannot supply GREEN for this scenario.

**GREEN.** Compose existing shared components/loaders behind the real BFF/session
and introduce the minimal test-only Playwright Node dependency inside the
existing Vitest runner. Reuse its deployed process/auth harness for later pages.

**REFACTOR.** Keep app code to host/layout/navigation state; retain all earlier
tests and shared Node/MCP owner boundaries.

### Scenario 5 — Logs can be filtered and opened as actual records

**Behavior.** A browser selects Logs, time-bounds/severity-filters the timeline
and records, wraps/pages/chooses columns, inspects actual body/attributes,
filters/excludes an attribute and opens the exact trace with return state.
Payload denial, absent values and generic names do not become fabricated data.

**RED.** Add planned V5 and missing shared component cases to V3. Demonstrate
the absent live Logs workflow and any missing controls against actual reads.

**GREEN.** Extend shared Logs controls/loaders once and compose them in the same
Observe host. Rerun earlier signal/browser scenarios affected by shared changes.

**REFACTOR.** Reuse record identity/navigation, severity fields and bounded query
owners; do not add page SQL or a separate Logs data service.

### Scenario 6 — Metrics preserve their recorded meaning

**Behavior.** A browser selects a metric and grouping, compares compatible
series, toggles a line from the series summary and changes range. Exact values,
units/kinds/temporality, missing versus zero and supported aggregates remain
truthful, including histogram/summary shapes. Unsupported plotting is explicit.

**RED.** Add planned V6 plus shared V3 precision/type/identity cases. Cover unsafe
integers, distinct units/temporalities, duplicate timestamps and nested counts
using real query projections; generic Number coercion cannot pass exact display.

**GREEN.** Add the minimum Metrics planning/shaping to shared owners and use the
existing transport/chart/table. Rerun Logs and Traces when shared code changes.

**REFACTOR.** Keep series identities typed and explicit, avoid implicit folding/
aggregation, and keep private helpers within the existing signal owner.

## Acceptance Criteria

| Obligations | Observation and proof |
|---|---|
| REQ-038; AC-021 | V1/V2 plus V9 demonstrate pinned canonical source/current production SDK, exact batch accounting, terminal verification and real spans/logs/metrics through normal dev startup. The canonical patch includes and proves batched Python log/metric/trace export while serving and drain on graceful shutdown; tracing alone is insufficient. No synthetic rows, paid inference or old provider double. |
| REQ-007/021/039; AC-022 | V4–V6 prove the complete shell/Observe workflow through real authenticated production-BFF/server reads. Every owned signal is navigable and populated in V9. |
| REQ-015/022/023/028; AC-008/017/018 | V3/V4 retain root versus flat spans, exact authorized evidence, recorded names, attribution/nulls, cancellation and read bounds; correlations never grant access. |
| REQ-016; AC-008/018 | V5 and component proof establish real Logs details, controls, severity and trace navigation with visible denial/error behavior. |
| REQ-017; AC-008/018 | V3/V6 establish Metrics chart/series interaction and lossless displayed values with correct recorded types. Raw points or guessed percentiles are failure. |
| REQ-040; AC-023 | V3 plus the recorded focused first-class validation journeys prove permission-first fragment validation, all projections, same predicate/binds and no dependent read after refusal. |
| REQ-020/030/034; AC-010/011/013/016 | V4–V8, recorded board/state/viewport comparisons and individual human acceptance close inherited R1/Observe obligations. A jsdom-only page or archived PASS claim does not complete this task. |
| INV-001–007 | Static diff/consumer audit and affected auth/query regression preserve contracts, all supported platforms, existing-owner design, credential/privacy/tenant boundaries and scope limits. |

## Implementation Reference

- `mise.toml::dev:full/dev:both`, `scripts/postgres/dev-server.sh`,
  `with-test-postgres.sh` and CLI init/process owners provide the real existing
  environment. They currently omit canonical app/batch and are ephemeral.
  Extend their complete startup/cleanup workflow; do not add a lifecycle service.
- Canonical `src/desk.py::Desk.handle`, `src/app.py`, `scripts/deploy.py`,
  `pyproject.toml`, and `test/test_api.py` establish Run/tool/verify/wait/vendor
  behavior. Current source has no batch driver, HTTP instrumentation or explicit
  application log/metric emission. Deliver counterpart changes in the pinned patch.
- `sdks/wyrd-sdk-python/python/wyrd/otel.py::start_telemetry` installs the trace/
  metric/log providers; canonical tests already use `Execution.connect(...).wait`.
  `tests/agent_readiness/live.py` and `check:py-wheel-no-testing` show production
  wheel construction; `py:setup` intentionally builds testing support instead.
- Branch-delivered `packages/ui/src/lib/observe.ts::{Binds,listQuery,facetQuery,
  detailQuery,waterfallQuery,loadRecordTable,loadRecordDetail}` and `schemas.ts`
  own bounded signal requests and normalized data. `/ui/load/+server.ts`,
  `node.ts::nodeExecutor`, existing `mcp.ts` and browser-session owners provide
  authorized IO. Main alone does not contain this component foundation.
- Existing `Chart`, `FacetRail`, `RecordTable`, `TraceWaterfall`, `RecordDetail`
  and `StatStrip` are the reuse owners. `plot.ts::timeSeries` is a generic
  numeric shaper, not a correct mixed-metric aggregate. Extend shared Metrics
  shaping/projection only for demonstrated missing semantics; pages have no SQL.
- `wyrd-queue/src/json_result.rs::JsonResultBuilder` and TS
  `BifrostQueryStream.collect` explain unsafe-integer JSON loss. Exact integer
  display can use existing SQL decimal-text projections and result types; prove
  nested-count projection support against real DataFusion before claiming it.
- `wyrd-server/src/query`/MCP Bifrost query owners, `wyrd-client::Bifrost` and
  `vala-bifrost-redux/src/tables/{traces,logs,metrics}` own validation, execution,
  columns and terminal behavior. Reuse the installed parser and described schema.
- `ui-library.integration.test.ts`, `vite.config.ts`, TS tests/support/server.ts
  and `identity_ui_e2e.rs::start_bff` provide real server/BFF/auth process patterns.
  Current jsdom/AppBridge/reqwest proof is not rendered-browser T6. Add Playwright
  Node API as a test-only dependency in the existing Vitest runner, not another
  runner, production dependency or test environment switch.
- [Retired R1](../../references/retired-task-004/TASK-004-R1-canvas-fidelity.md)
  preserves findings OBS-004-1–7 and component evidence. Retired R2/R3 do not
  authorize internal mock/provider/snapshot work. Full transferred page scope
  remains in TASK-012/020/021/022, now marked superseded.

## Verification and Evidence

New V1–V6 tests/files are **planned**, not existing PASS evidence. Existing
Vitest includes `src/**/*.{test,spec}.{js,ts}` and gates integration files through
the existing `WYRD_UI_INTEGRATION`. Confirm every actual selector after adding
the test and before RED; no-selected/skipped tests never pass. V10–V13 use
source-confirmed existing packages, targets and setup; only their new test
names are planned. Do not invent another test runner.

**Prerequisites.** Use `mise run ts:build`, `mise run ts:build:testing`,
`mise run build:server`, then
`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui install --frozen-lockfile`.
Build production BFF/library and provision Playwright Chromium through that
same project's test-only dependency for browser proof. The dev launcher builds
the production Python wheel; integration journeys may separately use the
repository's testing wheel/harness. Do not conflate those artifacts.

**V1 — development startup/batch lifecycle (planned browser-integrated test)**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^development environment lifecycle$"'
```

**V2 — canonical batch executor (planned counterpart test, prepared pinned checkout)**

```bash
mise exec -- uv run --directory target/dev/wyrd-example-py python -m pytest -q test/test_dev_workload.py::test_batch_reports_all_outcomes_and_retained_signals
```

**V3 — shared signal/predicate/component semantics (planned unit group)**

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.test.ts -t '^observe signal planning and interactions$'
```

**V4 — real browser Traces/Spans (planned)**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^authenticated traces and spans$"'
```

**V5 — real browser Logs (planned)**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^authenticated logs investigation$"'
```

**V6 — real browser Metrics (planned)**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^authenticated metrics investigation$"'
```

**V7 — existing shared Observe regression**

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-004: Canonical signal interactions$'
```

**V8 — existing authorized Node/MCP component regression**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^TASK-004: Authorized Observe across hosts$"'
```

**V9 — final normal-development replay; verification only, no manufactured RED**

```bash
mise run dev:full
```

Use the default 3,000-request run on final source, followed by an appended batch
through `dev:batch` while investigating live. Record source/patch/image/wheel
identity, URLs, actual counts/terminal outcomes, public retained readback and
browser screenshots/interactions for Traces, Spans, Logs and Metrics. No special
test server/flush enters this acceptance workflow. A fresh lifecycle is
ephemeral; do not add a dataset reset to make replay pass.

**V10 — public Rust predicate journey (planned, existing query integration owner)**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=query_bifrost::predicates_are_typed_bounded_and_tenant_scoped)"'
```

**V11 — real MCP predicate journey (planned, existing query pg_tests owner)**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=query::pg_tests::predicates_are_typed_bounded_and_tenant_scoped)"'
```

**V12 — public Python predicate journey (planned, existing query file)**

Use `mise run py:setup` for this testing-harness proof, separately from the
production wheel required by normal dev startup.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_query_bifrost.py::test_predicates_are_typed_bounded_and_tenant_scoped'
```

**V13 — public TypeScript predicate journey (planned, existing serverTest query file)**

Use the existing `ts:build`/`ts:build:testing` prerequisites above.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/query-bifrost.test.ts -t "^predicates are typed bounded and tenant scoped$"'
```

V10 exercises the Rust client's actual HTTP contract; V11 separately exercises
MCP. V12/V13 prove language projections and stable refusal/tenant behavior.
Include allowed expressions, malformed/injected input, unauthorized table
visibility, unknown columns, size bounds and normalized response shape. These
are focused journeys, not entire first-class SDK suites.

Run UI `check` and `build` through `mise exec -- pnpm --dir
crates/wyrd/wyrd-server/wyrd-ui`, plus `mise run check:py-wheel-no-testing`.
Use `mise run fmt`, `mise run lints`, `mise run py:format`, `mise run py:lints`,
`mise run py:typecheck`, `mise run ts:typecheck`, and `mise run codegen:check`
for their affected Rust/Python/SDK/catalog write sets. Add required native SDK
wiring/boundary checks when their projections change, through their owning
mise tasks. Finish with `git diff --check` and a full scoped diff audit. Do not
run broad family/user-journey aggregates or `gate` as a per-task checklist.

Record T1–T5 component results, automated real-browser T6, agent visual comparison
and **individual human acceptance separately** for Main, Spans, TraceDrawer,
TimePicker, Logs, Metrics and every inherited owned component. Include populated,
empty/loading/denied/error, desktop/mobile, light/dark, keyboard/focus, touch,
long content and overflow. No silent visual exceptions. Independent task-review
PASS plus these checkpoints closes this task and TASK-004; neither new file
creation nor batch HTTP success is completion.

## Material Stop Conditions

Stop for a required durable schema/migration, new query execution authority,
changed credentials/tenancy, unsupported public contract, lossless representation
requiring a new wire encoding, or an actual conflict with an owning architecture.
Report the exact evidence and approval needed. Do not omit a signal, replace
canonical source, invent metric semantics or broaden the mock to work around it.
Private helpers, equivalent selectors/setup, fixture matcher corrections and
ordinary local service recovery stay implementation-owned.

## Authority Links

- [Approved revision 18](../../spec.md), [order/ownership](../../plan.md),
  [verification owners](../../references/verification.md), [canvas](../../references/canvas/).
- [Repository rules](../../../../../AGENTS.md), [execution](../../../../implementation-execution.md),
  [workflow](../../../../spec-driven-development.md), [testing](../../../../../TESTING.md).
- [Wyrd design](../../../../../architecture/wyrd-design.md),
  [Bifrost](../../../../../architecture/bifrost-design.md),
  [security](../../../../../architecture/wyrd-security-posture.md),
  [UI patterns](../../../../../.agents/skills/wyrd-ui/SKILL.md).

## Execution Evidence

Implementation `4a7633111..bb524ffc9` on `task/wyrd-ui-main-integration/TASK-004`.
Independent review and human acceptance are still pending.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-038; AC-021: canonical source, current production wheel, batch accounting, retained signals | `mise.toml` `dev:full`/`dev:batch`; `scripts/dev/example-app.sh`; `scripts/dev/wyrd-example-py.patch` | V2 `test_batch_reports_all_outcomes_and_retained_signals` | PASS |
| REQ-038: one command reaches readiness, appended batch, Ctrl-C cleanup | `src/lib/ui-pages.integration.test.ts` `development environment lifecycle` | V1: refused before startup, because 8080/3000/8000 are held by an unrelated local service (`gif-guard-demo` wyrd-server 0.4.1, its UI, uvicorn) | NOT RUN |
| REQ-040; AC-023: permission-first fragment validation on every surface | `wyrd-server/src/query/{service,routes,floor}.rs`, `mcp/bifrost.rs`, `wyrd-client` Bifrost, Python/TS SDKs | V10, V11, V12, V13 `predicates_are_typed_bounded_and_tenant_scoped` | PASS |
| REQ-040: same predicate across charts/facets/list, no dependent read after refusal | `packages/ui/src/lib/observe.ts::predicateFor`; `schemas.ts` `where_sql` | V3 `observe signal planning and interactions` | PASS |
| REQ-007/021/039; AC-022: shell, Observe tabs, URL state, drawer, Query inspector | `src/lib/observe-host.ts`; `routes/t/[tenantKey]/{observe,query}/`; `+layout.svelte` nav | V3; V4 `authenticated traces and spans` | PASS |
| REQ-015/022/023/028; AC-008/017/018: roots vs spans, exact trace, invalid draft unapplied, denial, empty | Observe host, `RecordTable` `record.open_in_query` | V4 (production `node build` BFF, Chromium, real server) | PASS |
| REQ-016; AC-008/018: Logs timeline/severity, wrap/columns/paging, detail, Exclude into WHERE, Open trace and Back | `STATS` `logs`/`log_errors`; host Logs view | V3; V5 `authenticated logs investigation` | PASS |
| REQ-017; AC-008/018: Metrics series per recorded meaning, exact Latest/Max/Mean, unsafe integers, histogram listed and not plotted | `observe.ts::metricPanels`, exact `CAST … AS VARCHAR` projection; observe `+page.server.ts` predicate load | V3; V6 `authenticated metrics investigation` (mean `9007199254740994` from real SQL) | PASS |
| Shared Observe regressions | Moved OTLP seeding to `src/lib/ui-signals.integration.ts`; int metrics now decimal text | V7 `TASK-004: Canonical signal interactions`; V8 `TASK-004: Authorized Observe across hosts` | PASS |
| V9 final `dev:full` replay with screenshots | — | Not run: same port conflict as V1 | NOT RUN |
| INV-001–007 static checks | — | `mise run fmt`, `lints`, `py:lints`, `codegen:check`, `check:py-wheel-no-testing`, `pnpm run check`, full UI vitest (45), `git diff --check` (with `*.patch -whitespace`) | PASS |

Diagnoses:
- **Symptom:** `floor.rs` case `NAME in (...)` expected lowercase. **Evidence:**
  the normalizer preserves identifier spelling. **Cause:** the expectation was
  wrong; DataFusion folds unquoted identifiers at execution. **Fix site:** the
  test expectation.
- **Symptom:** UI vitest failed to load `examples/ui-app/.svelte-kit/tsconfig.json`.
  **Cause:** the example had never been synced. **Fix:** `svelte-kit sync`;
  environmental, no source change.

Limits and decisions:
- Playwright `1.64.1` is a test-only devDependency. Re-locking re-resolved the
  manifest's `latest` pins (svelte 5.57.3, vite 8.3.5 and others) at patch level.
- The metric chart and summary are authored SQL, which binds only the range.
  Metric name and WHERE values enter as escaped literals over allowlisted
  columns, and `where_sql` is server-normalized at page load first.
- Chart geometry is double precision, and its title says so. Nested
  `bucket_counts` stay JSON numbers.
- `record.logs` (span → logs) answers `unavailable`, because the logs filters
  have no trace or span column.
- Non-goals stayed excluded: no new wire encoding, no new query authority, no
  schema change, no page-owned data service.
