---
id: TASK-004-REV-02-R1
kind: remediation
status: proposed
spec: SPEC-wyrd-ui-main-integration
spec_revision: 18
parent_task: TASK-004-REV-02
remediates: [FIND-TASK-004-REV-02-1, FIND-TASK-004-REV-02-2, FIND-TASK-004-REV-02-3, FIND-TASK-004-REV-02-4, FIND-TASK-004-REV-02-5, FIND-TASK-004-REV-02-6, FIND-TASK-004-REV-02-7, FIND-TASK-004-REV-02-8, FIND-TASK-004-REV-02-9, FIND-TASK-004-REV-02-10]
---

# Restore the approved conventional Observe UI

The signed-in Observe workflow must look and behave like the approved Main,
Spans, TraceDrawer, TimePicker, Logs and Metrics boards, using the components
already delivered on this branch. Today the route presents a horizontal header,
ad hoc controls and incomplete signal compositions; its evidence panels appear
only after browser hydration. This remediation completes the agreed UI and
corrects the independently validated evidence and mandatory-gate defects in the
same task. It does not replace working data, auth, query or component owners.

## Assignment and authority

Execute through `$wyrd-implement` with `$wyrd-ui`. Read this entire task and the
original task before edits. The approved assignment is their combined scope;
this document resolves the correction boundaries, not a new product design.

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Approved specification: `changes/active/wyrd-ui-main-integration/spec.md`, revision 18.
- Original task: `changes/active/wyrd-ui-main-integration/tasks/TASK-004-revision/TASK-02-live-observe-workflow.md`.
- Original implementation base: `4a76331113864f9bddc43d3cc8df01080a696937`.
- Rejected reviewed candidate / next review's prior candidate: `f6aeb83a77179ac4450110671df6f564adc1914a`.
- Diagnosis and validation: adjacent `findings-validation.md`, discovery reports and `verdict.md`.
- Design inputs: `changes/active/wyrd-ui-main-integration/references/canvas/README.md`, six named `.dc.html` boards and their `png/` counterparts; inherited TASK-012/020/021/022 and `references/retired-task-004/TASK-004-R1-canvas-fidelity.md`.
- Mandatory owners: root/nested AGENTS, `changes/spec-driven-development.md`, UI skill and its SvelteKit reference, `agent_docs/maintainer-style.md`, security posture Permission check, and existing Bifrost query contracts.

Paths abbreviated as UI below start at
`crates/wyrd/wyrd-server/wyrd-ui/`. No source is authorized to change during
this review; this task is the handoff for subsequent implementation.

## Diagnosis and selected corrections

### 1. Canvas composition and reusable controls — FIND-TASK-004-REV-02-1

**Failure.** Spec REQ-007/015–017/020/021/039 and the original task explicitly
require the canvas shell and full signal pages. UI tenant `+layout.svelte:9–39`
keeps the horizontal header. Observe `+page.svelte:44–107` has separate native
range inputs, grouping/Live and WHERE rows; `observe-host.ts:153–175` substitutes
StatStrip for the three trace overview charts and severity timeline and supplies
a disconnected Metrics chart/summary. There is no reusable canvas TimePicker,
metric selector rail or complete trace list controls. Existing browser checks
assert data and a subset of controls; they never establish the board match.
Every signed-in investigation reaches these omissions, so this is acceptance
failure, not a discretionary visual polish request.

**Correction.** Compose the existing shared `Chart`, `FacetRail`, `RecordTable`,
`TraceWaterfall`, `RecordDetail`, `Drawer` and existing primitives directly into
the six approved board regions. Preserve working waterfall/detail behaviors and
the separately approved StatStrip consumers. Extend a shared component only for
an actually missing approved capability; never duplicate its table, chart,
waterfall, record detail, column/paging or selection workflow in page markup.

Build the missing shell controls once in the app: persistent left rail,
breadcrumb/top bar, reusable TimePicker, WHERE/group/Live controls. The
TimePicker is a reusable component consumed by all three Observe signals, with
preset and custom ranges, UTC/local timezone selection, validation, recent
committed ranges and the approved open-state interaction. Follow Main/TimePicker HTML structure/spacing and the current brand
tokens; illustrative values, obsolete attribution and reference hardcoded fonts
or colors do not replace current contracts or bundled brand assets. Preserve
login/logout, tenant/settings and themes; show undelivered destinations disabled
and keep Ask deferred.

Traces/Spans use three overview Charts, shared brush/zoom, folded/open Filters,
visible/held counts, Columns/CSV and the existing held-row/new-row/resume
behavior. Logs uses the actual severity timeline/facets and retained existing
wrap/columns/page-size/paging/detail/trace navigation. Metrics uses the board's
metric selector, recorded dimension grouping, supported aggregation controls,
Chart and selectable exact Latest/Max/Mean series table. Put series selection
and its typed props/actions in the existing shared component boundary so table
checkboxes and chart visibility identify the same series; keep one host-owned
selection state instead of independent legends. Supported controls must perform
their named operation. Unsupported percentiles/aggregations must be visibly
unavailable; do not invent percentile/rate semantics to fill a board.

A header restyle cannot close these missing interactions; extending existing
owners is smaller and safer than a new component kit or data service. Record
any unavoidable current-contract/brand difference explicitly for human review;
no silent visual exceptions are accepted.

### 2. Conventional SSR and one applied investigation state — FIND-TASK-004-REV-02-2

**Failure.** Spec REQ-030 Use case 1 requires server-loaded props and direct
component composition. Observe `+page.server.ts:13–22` returns only predicate
metadata; the page mounts `WyrdView` with `remoteLoader`. That renderer starts
with `panels=[]` and runs reads only in effects (`WyrdView.svelte:62,252–285`), so
initial HTML lacks evidence. Only RecordTable receives `live`; later list
refreshes slide independently of charts/facets/summary. Renderer-local brush
state never reaches the host URL (`observe-host.ts::step` has no chart.range),
so reload or navigation loses the investigation range. A working `/ui/load`
proxy does not satisfy conventional SSR or coordinated live semantics.

**Correction.** Use the existing authenticated Observe `+page.server.ts` to run
portable shared `loadPanel`/typed `load*` owners with `nodeExecutor` from the
session and return component props. Follow the existing conventional example
`examples/ui-app/src/routes/+page.server.ts` (repository root) and its direct-component page.
The app owns layout and navigation; the shared package still owns SQL,
dependent-read sequencing, result shaping and typed component behavior.

Use one applied URL state for range/timezone/predicate/facets/group/mode/exact
selection/return and one resolved absolute range/effective predicate per refresh generation for every chart, facet, list and metric summary. Route
TimePicker commits, brush, zoom/reset, signal/mode/group changes, exact record
selection and return links through that state and SvelteKit navigation or
invalidation. Preserve invalid draft refusal with the prior applied data and
no dependent reads after refusal. Bounded Live refresh uses that same owner;
keep existing RecordTable hold/new-row/resume behavior and reject stale results
from superseded navigation/refresh. Reload, Back and signal navigation must
restore the committed range and selection context.

Keep `/ui/load`, `WyrdView` and `remoteLoader` for their existing gallery,
view-document, custom-app and MCP consumers. Do not add preloaded results or
an SSR mode to the view-document contract; that duplicates an already existing
conventional path. No browser credential, page-owned SQL, second transport,
refresh framework or parallel query lifecycle is needed.

### 3. Recorded metric identity and truthful summaries — FIND-TASK-004-REV-02-3

**Failure.** UI `observe.ts:311–360` produces captions from kind/unit/temporality/
monotonicity plus optional stringified grouping, omitting recorded dimensions.
`plot.ts:38–63` then sums duplicates at the same timestamp and folds more than
20 series into “other”. The chart and raw summary consequently describe
different data; the implementation narrative about dropping duplicates is also
incorrect. Separate `COALESCE` integer/double max and mean aggregates ignore
doubles whenever any integer exists: samples 1 and 100 can show max=1/mean=1.
NULL and the real string `not set` also collapse in grouping. These ordinary
supported metric inputs violate the explicit no-implicit-folding requirement.

**Correction.** Extend the existing shared Metrics planner/shaper, not page SQL.
Keep an unambiguous typed series identity separate from display captions,
including compatible metric meaning and recorded resource/scope/point
dimensions; preserve NULL versus concrete values and current authorization for
sensitive fields. Feed that same identity and compatible sample set into Chart
and the selectable exact summary. Unaggregated metrics preserve independent
series and do not enter generic timestamp summation/overflow folding. Any
series bound must be visible and truthful, without silently synthesizing an
“other” metric. Preserve generic authored-query Chart behavior for its other
consumers rather than globally changing its established semantics.

When supported explicit aggregation/grouping is selected, apply that named
operation once over compatible samples and use its result consistently for
chart and summary. Compute Latest/Max/Mean across both supported integer and
double representations with exact display through existing SQL/result
capabilities; never choose between disjoint aggregates with COALESCE. Keep
approximate numeric chart geometry labeled and exact retained values visible.
Changing only captions or the legend cannot recover identity already lost by
the producer. No generic analytics engine or new transport is justified.

### 4. Complete, lossless recorded metric structures — FIND-TASK-004-REV-02-4

**Failure.** UI `observe-host.ts:50–51,151–165` selects a partial hardcoded
STRUCTURES inventory. Exponential histogram fields, summary sum and exemplars
remain unreachable; metrics has no record action or usable column selector to
reveal them. `observe.ts:289–302` casts scalar counts but passes nested uint64
counts as JSON numbers, losing values such as 9007199254740993. Small `[1,2]`
fixtures and scalar precision checks do not prove the required structures.

**Correction.** Reuse the shared Metrics schema/`signalColumns` and existing
RecordTable presentation to expose all required recorded histogram, exponential
histogram, summary/quantile and exemplar structures. Preserve scalar kinds,
units, temporality, monotonicity and missing-versus-zero. Extend shared SQL
projections through the existing query owner to return nested counts as exact
decimal text with their structure intact. Prove that projection against real
SQL before claiming it works. Unsupported plots remain explicit, while exact
recorded structures stay inspectable. A new raw-detail service, Arrow path or
JSON wire encoding is prohibited. If existing SQL cannot supply required exact
structures, report the original material stop condition with concrete evidence;
never substitute silent rounding or hide the shape.

### 5. Setup failures must leave useful services inspectable — FIND-TASK-004-REV-02-5

**Failure.** REQ-038 preserves healthy services after setup/batch failure.
`mise.toml:1323` runs canonical setup bare under `set -e`, after server/UI/mock
startup. A deploy/Card declaration failure triggers EXIT cleanup and destroys
the ephemeral environment, whereas batch failure is already guarded. Happy-path
startup evidence never exercises this required negative branch.

**Correction.** Extend the existing dev:full setup/batch status handling so setup
failure is visible and keeps healthy server/UI/MockServer running for diagnosis.
Do not launch dependent app/batch or claim their readiness after failed setup.
Retain the existing owned-process cleanup on Ctrl-C and early startup failures
where useful dependencies do not exist. Reuse the current guarded batch/status
pattern; add no supervisor, retry policy, persistent environment or reset mode.

### 6. Batch completion must prove actual request membership — FIND-TASK-004-REV-02-6

**Failure.** The canonical patch `scripts/dev/wyrd-example-py.patch:88–198`
removed each Call's trace identity when it correctly stopped injecting fabricated
parent spans. Readback now accepts all support-desk HTTP/log traces since batch
start and ambient judge totals. Unrelated traffic in this intentionally live
environment can replace a missing workload span/log and still pass. Returned
Run verdicts do not prove HTTP-only/error request telemetry. The task expressly
requires actual trace/Run correlation and failure on missing evidence.

**Correction.** Keep plain requests and app-owned root spans. At the existing
canonical app ASGI instrumentation response hook, return the actual active OTel
trace context as ordinary application response metadata for its batch client;
record it for all four workload cases, including no-Run HTTP/error responses.
Use this actual per-call identity and existing returned Run/execution IDs to
constrain public Bifrost retained-evidence and judge-attempt membership. Keep
metric observations separately reported as time-window observations; metric
points are not one-per-request evidence. Do not restore a made-up traceparent,
add a second Run/exporter, invent grouping metadata or introduce Wyrd SDK/header
selection, server schema, permission or query changes. Aggregate thresholds
cannot recover discarded membership; the small canonical app/driver context
bridge is the correction boundary. Preserve report denominators and existing
request/verification lifecycle.

### 7–10. Batch mandatory boundary corrections

| Finding | Diagnosis and exact owner | Selected correction and limit |
|---|---|---|
| FIND-TASK-004-REV-02-7 | New native `sdks/wyrd-sdk-python/src/bifrost/mod.rs:979::validate_predicate` has no authored declaration in `python/wyrd/_wyrd/bifrost.pyi`; Any fallback hides parity failure despite green checks. Python AGENTS requires the declaration. | Add only the exact native method signature and parameter/return docs to the existing native Bifrost stub. Keep public `_client.py` classes and re-exports single-sourced; no fallback cleanup. |
| FIND-TASK-004-REV-02-8 | New fallible `query/floor.rs:307::check_predicate` and `:408::check_column` lack mandatory `# Errors` docs. | Complete those existing doc blocks with role, invariant and rejection/stable-error behavior. No parser behavior change or style-only round. |
| FIND-TASK-004-REV-02-9 | New public `query/routes.rs:191::validate_predicate` lacks mandated scrubbed `tracing::instrument`. | Apply existing instrumentation with state/caller/body and predicate contents skipped; use only safe existing correlation fields. No raw filter logging or new telemetry schema. |
| FIND-TASK-004-REV-02-10 | `query/service.rs:346–347` directly reads effective_permissions instead of the configured PermissionCheck for resolved table and payload decisions. This violates the security authority; current RBAC is equivalent, so no privilege escalation is claimed. | Call existing `state.auth.permission_check.check` for each already-resolved scoped Permission and payload Permission. Preserve coarse admission, scoped grants, denial-before-description, stable refusal mapping and existing audit. Do not call global-only QueryAuthority::authorize unchanged; it rejects valid table-scoped readers. No new checker/helper authority. |

## Preserved behavior and excluded work

Preserve existing canonical source/current production wheel pinning, normal OTel
export/drain, app Run ownership, root-versus-flat spans, exact authorized trace
retrieval including out-of-list-range/untagged spans, attribution/nulls, typed
identities, shared Node/MCP loader parity, permission-first predicate validation,
last valid WHERE draft behavior, exact SQL/binds Query inspection, bounded
reads/cancel/terminal/partial/refusal states, tenant isolation and all supported
platforms. Preserve shared components and previously proven interactions.

No new durable schema, migration, wire encoding, dependency, component kit,
backend/query service, mock provider, credential authority, lifecycle controller,
Ask/saved views, unrelated destination page, complete Query workbench, persistence
promise or speculative hardening. Private helpers, local control flow and
fixture organization remain implementation-owned. The original task's material
stop conditions remain binding.

## Acceptance and focused proof

Implement one behavior at a time with real RED/GREEN evidence in existing tests;
static docs/declaration obligations need inspection, not artificial tests.
Do not weaken assertions to clear a failure. Record cause and evidence for any
test correction. Planned additions below extend the existing named tests; confirm
the actual selector before running RED and do not claim skipped tests as PASS.

| Findings | Required observable closure |
|---|---|
| 1 | Main/Spans/TraceDrawer/TimePicker/Logs/Metrics match their approved board regions and interactions using existing components. One reusable TimePicker serves all signals. Series-table selection and chart lines agree. Record side-by-side reference/candidate at desktop reference size and narrow/mobile, both themes, including populated/loading/empty/denied/error/partial, keyboard/focus, touch, long content/overflow. Record each component/board's proof and individual human acceptance separately; the implementor cannot grant that acceptance. |
| 2 | An authenticated initial HTML response, including exact-record URLs, contains permitted signal content or truthful terminal state before JS. Hydration does not replay ownership of all initial loads. One refresh generation uses identical range/predicate bounds for every affected panel; stale results cannot overwrite. Brush/custom range/zoom/reset/Back/reload/signal switch preserve state; Live holds/new-row/resume still work with an appended observation. |
| 3 | Same-time different dimensions/resources stay separate; typed NULL/string grouping remains distinct; mixed int/double values produce correct exact summaries; >20 series causes no implicit metric folding. Plot, selected-series table, Latest/Max/Mean and visibility share identities/sample sets, with only labeled geometry approximation. Generic authored-query Chart consumers retain their behavior. |
| 4 | Real SQL returns and UI shows exact unsafe nested counts, explicit and exponential histogram shapes, summary quantiles/sum and exemplars, with missing versus zero preserved and unsupported plots disclosed. Small numeric fixtures alone cannot close this. |
| 5 | Force canonical setup failure after dependency readiness: failure visible; app/batch not started; server/UI/mock still usable. Ctrl-C then cleans up only owned children. |
| 6 | Small real batch plus unrelated traffic: every workload case retains actual app-owned trace identity, roots stay roots, each required span/log and judge attempt maps to the expected trace/Run/execution. Remove one workload evidence item in the readback check and prove unrelated rows cannot replace it or hide the specific failure. Denominators stay exact. |
| 7–9 | Native/stub signature parity and required docs/instrumentation are visible in source; checks stay green and predicates never enter traces. |
| 10 | Both decisions use configured PermissionCheck; existing scoped success, ungranted-table refusal before schema disclosure and payload distinction remain correct on public Rust and MCP paths. |

Use the existing Vitest/Playwright, canonical pytest and BFF/server fixtures;
no new runner or synthetic host load. Extend the existing focused tests rather
than requiring full suites:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.test.ts -t '^observe signal planning and interactions$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-004: Canonical signal interactions$'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^authenticated (traces and spans|logs investigation|metrics investigation)$"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^development environment lifecycle$"'
mise exec -- uv run --directory target/dev/wyrd-example-py python -m pytest -q test/test_dev_workload.py::test_batch_reports_all_outcomes_and_retained_signals
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=query_bifrost::predicates_are_typed_bounded_and_tenant_scoped)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=query::pg_tests::predicates_are_typed_bounded_and_tenant_scoped)"'
```

The focused lifecycle case uses a small bounded workload and its setup-failure
branch, not another 3,000-request replay. Canonical pytest requires the pinned,
patched checkout and production-wheel preparation from the original task. UI
browser proof requires the existing built Rust server, TS native/testing
artifacts, production BFF and installed Chromium. Reuse the original documented
mise prerequisites; the testing harness and production wheel stay distinct.

Additional exact checks for the affected write set:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run py:lints
mise run py:typecheck
mise run fmt:check
mise run lints
mise run codegen:check
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^TASK-004: Authorized Observe across hosts$"'
```

The last command covers affected authorized Node/MCP loader/prop parity. Run
codegen checks for changed catalog/schema inputs and regenerate through the
existing owner. Rust lint scope follows the mandatory repository task; do not
invent another lint environment. No `gate`, full
SDK/journey suites, family aggregates or default V9 replay is required here;
those belong to integrated change review. Finish with scoped diff/consumer audit
and `git diff --check`.

Attach correction evidence to this task without changing the immutable prior
reports. The next task review checks these finding closures and affected
consumers against the original task and cumulative candidate; it does not restart
unrelated discovery. The UI remains unaccepted until the separate required
human visual checkpoints are actually recorded. Do not commit or push unless
asked.

## Remediation evidence

Commits: `8d1ed2d35` (F7–F10), `38698d61b` (F5–F6), `f30cc7219` (F1–F4
implementation), `1015bc024` (F1–F4 journeys), `7aeaa7f7d` (lifecycle headline
read), `566dca7e6` (narrow-screen layout). Status: `IMPLEMENTED` with one
recorded material stop condition (exemplar integer precision, below). Human
visual acceptance is pending for every board; the implementor has not granted it.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| F1 canvas shell and boards from existing components | `routes/t/[tenantKey]/+layout.svelte` left rail (undelivered destinations and Ask disabled, identity menu); `observe/+layout.svelte` tabs, toolbar, Traces overview/listbar/facets/RecordTable, Logs timeline, Metrics rail/series/list; `observe/+page.svelte` Drawer + TraceWaterfall, log RecordDetail aside | integration `^authenticated (traces and spans\|logs investigation\|metrics investigation)$`; screenshots below | PASS (visual acceptance pending) |
| F1 one reusable TimePicker for all signals | `src/lib/TimePicker.svelte` (presets, custom From/To with `now`/`now-2h`/`yesterday`/ISO, UTC/Local, validation, Recent) | `ui-pages.test.ts` readInstant/presetLabel/rangeLabel; journeys: invalid, custom, preset paths on traces/logs/metrics | PASS |
| F1 series selection agrees with chart lines | `Chart.svelte` host-owned `shown`/`onseries`, checkbox reset to drawn state | `ui-library.test.ts` `^TASK-004: Canonical signal interactions$` | PASS |
| F2 permitted content or terminal state in first HTML | `observe/+layout.server.ts`, `observe/+page.server.ts` conventional loads | journeys read `serverHtml` before JS for list, exact trace and log URLs | PASS |
| F2 one range/predicate per refresh; no stale overwrite | layout load resolves one range per `depends('wyrd:observe')` run; SvelteKit discards superseded navigations; Live invalidates | journeys: zoom 03:00→09:00, reset, Back, reload, signal switch, Live append | PASS |
| F2 malformed range refused | `RangeSchema.safeParse` → `inputProblem`; `zonedText` handles invalid instants | journey: `WYRD_SPEC_400_VALIDATION` visible, no 500 | PASS |
| F3 recorded series identity, NULL vs string, mixed int/double, >20 series | `observe.ts` `metricQueries` identity + exact Latest/Max/Mean SQL; `recordedSeries` without fold | `ui-library.test.ts` 24-series, NULL/`not set`/`{}`, 9007199254740993 summary; journey legend 5 series, `max` → 2, `by service_name` | PASS |
| F3 generic Chart consumers unchanged | Chart identity props optional; authored-query path untouched | `ui-library.test.ts` full file | PASS |
| F4 exact nested counts and all structures reachable | `observe.ts` `NESTED_EXACT` (`CAST(... AS VARCHAR[])`, `named_struct`); Metrics `signalColumns` + RecordTable `oncolumns` → URL | journey seeds unsafe ints, explicit/exponential histograms, summary quantiles/sum, exemplar; columns revealed via URL and Columns menu | PASS except exemplar `int_value` precision (stop condition) |
| F5 setup failure keeps services | `mise.toml` dev:full guarded setup status | integration `^development environment lifecycle$` asserts `setup FAILED`, app not started, batch not sent | PASS |
| F6 per-request membership | `scripts/dev/wyrd-example-py.patch` `traceresponse` + `missing()` | `scripts/dev/example-app.sh test -q test/test_dev_workload.py::test_batch_reports_all_outcomes_and_retained_signals` | PASS |
| F7–F9 stub, `# Errors`, scrubbed instrument | `bifrost.pyi`, `query/floor.rs`, `query/routes.rs` | `py:typecheck`, `py:lints`, `lints`, source inspection | PASS |
| F10 configured PermissionCheck | `query/service.rs` | `wyrd-ai` `query_bifrost::predicates_are_typed_bounded_and_tenant_scoped`; `wyrd-mcp` `query::pg_tests::predicates_are_typed_bounded_and_tenant_scoped` | PASS |
| Affected Node/MCP loader parity | unchanged loaders | `ui-library.integration.test.ts` `^TASK-004: Authorized Observe across hosts$` | PASS |
| Write-set checks | — | `pnpm check` (package, app), UI build, `fmt:check`, `lints`, `codegen:check`, `py:lints`, `py:typecheck`, `git diff --check` | PASS |

Non-goals stayed excluded: no new schema, migration, wire encoding, dependency,
component kit, query service, Ask or saved views. Overview and GenAI tabs and
Cards/Verify/Changes destinations are visible and disabled.

### Failure diagnoses

- **SSR 500 on malformed `to`.** Symptom: `?to=bad` returned 500. Cause: the
  TimePicker trigger label called `toISOString()` on `NaN`. Fix site:
  `zonedText`, the one formatter all labels use; it now reads "invalid time" and
  the load's typed refusal renders.
- **Native checkbox desync.** Symptom: a refused or host-owned toggle left the box
  checked while the line or column was hidden. Cause: browsers flip `checked`
  before the handler runs. Fix sites: `Chart.svelte` resets to the drawn state;
  `RecordTable.svelte` reverts only a refused toggle (the last column).
- **Lifecycle trace total.** Symptom: lifecycle journey could not read a total.
  Cause: the regex matched the removed StatStrip text. Fix: read the overview
  headline `N traces`; no assertion weakened.
- **Batch pytest under the production wheel.** Symptom: `wyrd.testing` import
  error. Cause: the remediation command runs the production wheel; the test
  harness lives in the testing wheel. Fix: run through
  `scripts/dev/example-app.sh test`, which installs the testing wheel; production
  wheel pinning unchanged.
- **Integration locators.** Columns is `<details><summary>`; disabled destinations
  are `[aria-disabled="true"]`; group selects carry explicit `aria-label` because
  a wrapping label includes the selected value; after Back the test waits for
  rows. Each corrected the locator, not the assertion.
- **Narrow layout.** Screenshots showed the rail widened by a long subject id,
  wrapped signal tabs, wrapped metric types and the Calculation control
  overflowing 390px. Cause: missing `min-width: 0`/`nowrap`/wrap rules. Fixed in
  the owning layouts; every mobile capture is exactly 390px wide.

### Material stop condition: exemplar integer precision

`exemplars` is `List<Struct<time_unix_nano, int_value Int64, double_value,
filtered_attributes Variant, trace_id, span_id>>`. Through existing SQL:

- `exemplars` as-is: `filtered_attributes` reads correctly; `int_value` crosses
  as a JSON number and loses values above 2^53.
- `CAST(exemplars AS VARCHAR)` / `arrow_cast` to `int_value Utf8`: `int_value`
  exact, but `filtered_attributes` becomes raw Variant `metadata`/`value` binary.
- `arrow_cast` with `filtered_attributes Utf8`: refused (Variant to Utf8 is not
  castable).
- `unnest` + per-field cast: exact, but loses the per-point list structure the
  task requires to stay intact.

No existing SQL capability returns exact exemplar `int_value` with readable
attributes and the structure kept. Per F4 the projection stays unchanged (shape
and attributes visible, exemplar integers above 2^53 not exact) and this is
reported, not hidden. Closing it needs a server-side Variant-to-JSON or exact
nested int64 projection, which is outside this remediation.

### Visual evidence

Candidate captures were taken outside the repository (never committed) for
Main, Spans, TraceDrawer, TimePicker, Logs, Metrics, empty and error states, in
desktop (1440×1000 viewport) and mobile (390 full page), dark and light, against
`references/canvas/png/{Main,Spans,TraceDrawer,Logs,Metrics}.png`. They come from
`dev:full` with a 40-request batch. Denied, partial, keyboard/focus and long-content
states are proven by the journeys, not captured.

Recorded differences from the reference:

- Columns/CSV/Wrap sit in the RecordTable header (the existing component), not the
  listbar row.
- Metric legend captions show the recorded identity cells verbatim (JSON
  attributes); long identities truncate.
- The metric rail lists one row per name + type + unit.
- On mobile the trace Drawer is viewport-fixed; full-page captures show it partway
  down the page.
- p50/p90/p99 calculations are visible but disabled (no fabricated percentiles).

| Board | Component proof | Human acceptance |
|---|---|---|
| Shell / rail | journeys (destinations, identity menu) | pending |
| Main (Traces) | traces journey | pending |
| Spans | traces journey | pending |
| TraceDrawer | traces journey (exact URL, SSR) | pending |
| TimePicker | unit + three journeys | pending |
| Logs | logs journey | pending |
| Metrics | metrics journey + library test | pending |
| States | malformed range, empty range, denied viewer journeys | pending |
