---
id: TASK-004-R1
kind: remediation
status: superseded
spec: SPEC-wyrd-ui-main-integration
spec_revision: 14
requirements: [REQ-020, REQ-022, REQ-023, REQ-028, REQ-030, AC-008, AC-010, AC-011, AC-016, AC-017, AC-018]
depends_on: [TASK-003]
parent_task: TASK-004
remediates: [OBS-004-1, OBS-004-2, OBS-004-3, OBS-004-4, OBS-004-5, OBS-004-6, OBS-004-7]
---

> Retired on 2026-11-02 by the caller-directed revision 18 replacement.
> Historical instructions and evidence only; do not execute this task.
> Current owners: [Task 01](../../tasks/TASK-004-revision/TASK-01-mockserver.md)
> and [Task 02](../../tasks/TASK-004-revision/TASK-02-live-observe-workflow.md).
> No historical implementation claim becomes review PASS or visual acceptance.


# TASK-004-R1 — Make the implemented Observe components inspectable and faithful

## Outcome and Value

Repair the existing TASK-004 implementation against approved spec revision 14.
The maintainer can inspect each component beside its approved canvas region,
exercise its states, and see which obligations are fixed or still unresolved.
The original task's implementation evidence remains historical evidence; this
task does not rebuild the family or declare it accepted.

For example, RecordDetail currently prints message attributes as JSON. After
remediation its span Messages view shows the recorded roles and content in the
TraceDrawer message layout, while Raw preserves the returned payload.

## Authority and Findings

The caller requested remediation on 2026-10-29. These are source-validated
observations at `d00a9834c50de51629804c3ad6a508f2fb221cc8`, not a formal
immutable task-review verdict. Paths below are relative to
`crates/wyrd/wyrd-server/wyrd-ui/`.

| Finding | Source and failing observation | Required outcome |
|---|---|---|
| OBS-004-1 | `src/routes/_gallery/+page.svelte:131–164` directly assembles Observe components and mixes their states; only the separate example at 184–185 uses WyrdView. | Every owned component and required state renders through fixture view documents and WyrdView, with its board and region labelled. |
| OBS-004-2 | `packages/ui/src/lib/components/FacetRail.svelte:23–50` omits Main's WHERE hint and substitutes explanatory text for the board's expansion affordance. | Match the facet region and value-selection interaction; retain truthful count/limit behavior and approved Workload/Card labels. |
| OBS-004-3 | `packages/ui/src/lib/components/RecordTable.svelte:100–168` has a paragraph live notice, generic service text and no board footer. | Match list geometry, chips and live notice. A footer must describe the actual returned extent and any known total without inventing a count. |
| OBS-004-4 | `packages/ui/src/lib/components/TraceWaterfall.svelte:64–98` lacks TraceDrawer's axis, collapse caption and collapsed-child counts. | Restore timeline guidance and derived child counts while preserving tree selection, expansion and error ancestry. |
| OBS-004-5 | `packages/ui/src/lib/components/RecordDetail.svelte:38–125` shows message JSON, uncounted tabs and lacks the populated exception banner/attribute side region. | Match TraceDrawer's role blocks, truthful tab counts and populated detail regions using recorded fields. Preserve raw access, denial and absent-payload behavior. |
| OBS-004-6 | `packages/ui/src/lib/components/RecordTable.svelte:126–153` interprets authored SQL aliases as canonical headings, statuses and duration/timestamp fields without a signal input. | A returned alias alone must not turn a number into an error badge or duration, or assign telemetry meaning/colour. Preserve authored query headings and values. |
| OBS-004-7 | `packages/ui/src/lib/components/RecordTable.svelte:69–75` keys groups by formatted text. Distinct nanosecond values can round to the same duration; null and literal `∅` also collide. | Preserve typed source values as group identities. Display captions must not merge distinct groups. |

## What Changes and What Must Stay True

Reuse the five existing components, `observe.ts` loaders, catalog, fixture
executor, WyrdView and existing tests. Repair their shared owners and affected
consumers once. Use the repository `wyrd-ui` skill for implementation.

| Component | Approved visual comparison |
|---|---|
| FacetRail | Main and Spans left facet column; Logs left facet column. |
| RecordTable and reused Chart | Main trace list, Spans flat list, Logs records list; Metrics selectable series summary and chart-line visibility. |
| TraceWaterfall | TraceDrawer upper span tree and timeline. |
| RecordDetail | TraceDrawer lower selected-span detail; Logs opened-record body. |
| StatStrip | Agent Invocations/Model usage, Service Signals, Home Observe summary; record the inventory/board conflict below before changing its published inputs. |

Keep every required populated/loading/empty/denied/error state together under
its component checkpoint; include live, missing counts/payloads, long content
and narrow-container cases where applicable. Labels link to the board PNG and
identify the exact region. Fixture provenance follows REQ-030; canvas sample
values are illustrative and are not a substitute for recorded fixture data.

The canvas governs appearance; the approved spec governs truthful data and
documented canvas corrections. Span names come from data. Generic trace rows
and waterfall bars gain no classification keyed to example operation names.
The current waterfall already colours by status; this task does not claim that
the proposed operation-name colouring was implemented. The separately approved
Agent invocation statistic remains valid. Preserve exact identities, range,
authorization, sensitive-payload refusal, missing values and currency separation.

The original TASK-004 scope includes metric variants. Close reusable Metrics
series-summary/selection behavior in the existing shared Chart/RecordTable and
portable loader owners before the full-library gate. Prove Latest/Max/Mean
where valid for the recorded metric shape, stable series identity and matching
chart-line visibility; unsupported aggregates remain unavailable. If existing
shared capabilities already suffice, record their exact verification instead
of adding code. TASK-022 only composes this accepted behavior into the page.

Likewise, close missing reusable Logs wrap, bounded paging/page-size, column
controls and attribute Filter/Exclude/Open trace behavior here, through existing
component/loader/action owners and their T1–T5/gallery proof. TASK-021 owns
route composition, authorized host wiring and T6 after shared acceptance.
Known missing shared behavior is not deferred until page implementation.

No new Observe pages, telemetry schema, server workflow, transport, framework
or dependency. Reuse Chart and primitives. Preserve existing passing behavior;
rerun proof affected by changed props, loaders or generated contracts.

## Approach

1. Map the five gallery checkpoints to the regions above and identify every
   explicit spec correction before changing appearance.
2. Repair gallery rendering and the confirmed component omissions in shared
   owners; preserve working interactions and truthful data states.
3. Record any actual conflict with its two sources and affected obligation;
   continue independent fixes without inventing replacement semantics.
4. Run focused behavioral proof, capture labelled visual comparisons and
   submit each component for individual human inspection.

## Generic Data Across Every Host

The conventional UI, self-service/custom app and MCP App consume the same
shared workbench components. Audit every runtime rendering, formatting and
classification path in the UI package and its app consumers for hardcoded
span, service, model, tool, metric and attribute-value rules. Record the
inspected owner, the condition and its approved semantic source; remove
unsupported mappings at the shared owner. A search for a few example names
alone does not close this obligation.

Names and arbitrary labels remain data. Renaming a record or adding an unseen
name must not change its colour, icon, status, inferred operation category,
visibility or available interactions. Chart series may use the generic palette
and non-colour cues to distinguish series, without assigning domain meaning
to their labels. Literal interface labels and explicit canonical schema/status,
severity, recorded verdict and message-role semantics are allowed; their source
must be the typed input or actual field, never a guessed name or substring.

Ordinary HTTP/client/server/internal spans and traces are first-class inputs.
Do not require GenAI operation/model/message attributes, a known service name,
Wyrd Card attribution or optional HTTP attributes to render a retained span.
Display the recorded name as supplied. Missing optional fields and payloads
stay absent, with truthful empty states; never synthesize roles, model/tool
counts, a timeout explanation or another value from a name. Required canonical
schema columns are distinct from optional observation attributes.

An authored SQL alias does not declare canonical telemetry semantics. Preserve
returned headings and values; an alias such as `status_code` or `duration_nano`
must not automatically produce an error badge, duration conversion or semantic
colour. Canonical signal inputs may retain their documented field formatting.
Retain the specifically approved Agent invocation statistic without promoting
that operation predicate into generic trace presentation.

Prove arbitrary/unseen record and series names, misleading names such as
`status.error`, and query aliases that collide with canonical fields. Include
the shared WyrdView path used by the hosts, unchanged exact-value filtering and
selection, and the converse: changing an actual status changes status rendering
independently of the name. Carry this proof through affected built host outputs.
Group identity likewise uses typed source values, never formatted captions;
renaming or formatting cannot merge distinct groups.

## Ordered Implementation Scenarios

### Scenario 1 — Individually inspectable component states

**Behavior.** Every owned component has a board/region-labelled gallery
checkpoint. Its required states render through WyrdView fixture documents in
both themes, with usable keyboard interaction and narrow layouts.

**RED.** Extend the existing canonical-interactions test with missing gallery
document/state coverage and observable affordances; run V1 below and confirm
the missing behavior fails. Static labels and CSS need visual/static proof,
not manufactured runtime failures.

**GREEN.** Repair the existing gallery, fixtures and shared component owners.
Repeat V1 after each cohesive behavior change.

**REFACTOR.** Remove duplicate assembly or styling where existing owners
suffice; retain all earlier assertions and host boundaries.

### Scenario 2 — Faithful record investigation using recorded data

**Behavior.** Facet selection, held/live list inspection, waterfall expansion
and selected-record detail retain their current semantics while displaying
the approved board regions. Role blocks, counts, axis positions and exception
details derive from actual inputs. Metrics series summary/selection matches
the chart and preserves actual kind, unit, temporality, precision and missing
values. Missing or refused data stays explicit.

**RED.** Add focused assertions to the existing canonical-interactions test
for the changed non-trivial behavior and run V1. Where a loader/data shape
changes, extend the existing authorized-host test and run V2 to expose the
missing real-data behavior.

**GREEN.** Implement the smallest owner correction and rerun the affected
earlier assertions. Update generated catalog projections through their owner
only when published inputs actually change within approved authority.

**REFACTOR.** Preserve exact trace navigation, host action dispatch, cancelled
loads and sensitive-payload refusal while removing redundant code.

## Acceptance Criteria and Evidence

| Obligations | Observation and proof |
|---|---|
| REQ-020/030; AC-010/011 | For each component, record board/region, state, theme, viewport, reference crop, implementation screenshot and discrepancy disposition. Compare populated dark appearance against the approved PNG/HTML; inspect light/mobile for the approved structure, tokens and interactions. An overflow screenshot alone cannot pass fidelity. |
| REQ-022/023/028; AC-008/016–018 | V1/V2 prove affected data semantics: names from input, canonical fields, exact identity/time, null versus zero, permission refusal, retained raw payload and no invented classification or values. |
| OBS-004-1–7 | Each confirmed omission or interpretation defect closes with its component behavior and applicable visual evidence. Every difference is either fixed or linked to an explicit human-approved exception; an unresolved difference remains unaccepted. |
| Generic host semantics; REQ-022/030 | Runtime condition inventory plus adversarial-name and colliding-query-alias regressions show that returned names do not define meaning. All hosts reuse the corrected owner; a reserved-name case, guessed classification or name-dependent status rendering fails this obligation. |

Record automated results, agent visual comparisons, and human acceptance
separately. Obtain human acceptance for each component in desktop/mobile and
light/dark before dependent work proceeds. Existing test PASS records do not
establish new visual acceptance.

## Verification and Evidence

Use the original task's [verification setup](../verification.md).
V1/V2 exist in the inspected test owners; V3/V4 are planned additions. Extend
these owners rather than introducing a new runner or test setting.

**V1 — changed component/gallery behavior:**

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-004: Canonical signal interactions$'
```

**V2 — affected real data, authorization and built host artifact:**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^TASK-004: Authorized Observe across hosts$"'
```

**V3 — planned regression: authored query aliases retain their returned meaning:**

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-004: Authored query aliases do not declare telemetry semantics$'
```

**V4 — planned regression: record names do not drive telemetry presentation:**

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-004: Telemetry presentation is independent of record names$'
```

Confirm V3/V4's actual selectors when implemented. V4 includes ordinary HTTP
spans without GenAI attributes, missing service/Card attribution, arbitrary
series and record names, exact-value filtering/selection and status changes
independent of names. Extend V1 for typed grouping collisions: null versus
literal `∅`, and different durations with identical formatted captions.

Run the existing UI `check` and `build` commands through mise; run
`mise run codegen:check` if source catalog/theme/generated inputs change.
Run the gallery through the original task's normal development command.
Record source/browser identity and actual selected-test results. Carry forward
unchanged proof only with an explicit reason. This document specifies planned
verification; no remediation implementation or new regression tests are
retained in the audited runtime source.

## Material Stop Conditions

The inventory describes StatStrip as numbers with sparklines, while Agent,
Service and Home show different summary presentations. Shared Metrics behavior closes here before library acceptance; TASK-022 owns
page composition. The StatStrip conflict remains recorded in the [remaining-task audit](../task-audit-2026-10-29.md).
This task cannot silently choose a new published variant contract, abandon a
required board region, or declare the StatStrip conflict accepted. Resolve their
approved interpretation before calling affected component work complete.

Expansion/footer work must establish whether current bounded inputs can supply
the displayed extent. Never invent totals, treat a truncated result as complete,
or add a public pagination/filter contract without the required authority.
A real conflict blocks its affected obligation; independent fixes continue.

## Authority Links

- [Approved spec revision 14](../../spec.md), especially REQ-020/022/023/028/030.
- [Original TASK-004 and retained implementation evidence](../../tasks/TASK-004-observe-components.md).
- [Canvas and provenance](../canvas/README.md).
- [Execution](../../../../implementation-execution.md) and [spec-driven workflow](../../../../spec-driven-development.md).

## Implementation Evidence

Candidate: uncommitted working tree on `task/wyrd-ui-main-integration/TASK-004`
over `d00a9834c`. Paths below are under `crates/wyrd/wyrd-server/wyrd-ui/`.
Status: **IMPLEMENTED**, pending `$wyrd-task-review` and human visual acceptance.

TDD note: the V1 extension and the V3/V4 tests were written after the
component changes, not first. Each was confirmed against the changed
behavior, and two defects that jsdom cannot observe were found only by the
browser comparison below. RED-first evidence therefore does not exist for this
remediation.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| OBS-004-1 every component/state through WyrdView fixture docs, board/region labelled | `src/routes/_gallery/+page.svelte` (checkpoints: FacetRail, RecordTable, Chart/Metrics, TraceWaterfall, RecordDetail, StatStrip × populated/empty/loading/denied/error/narrow, light and dark); `src/routes/_gallery/board/[name]/+server.ts` (dev-only board PNG); `src/lib/ui-fixtures.ts` `stateExecutors`, `liveExecutor` | headless Chrome capture of `/_gallery` at 1440 light, 1440 dark, minimum (500 px) viewport; all 9 board links return 200 | PASS (agent); human acceptance pending |
| OBS-004-2 FacetRail WHERE hint, "+ N more" expansion | `packages/ui/src/lib/components/FacetRail.svelte` | V1: hint, `+ 5+ more`, expanded values, "More values exist; filter to narrow them.", status error tone | PASS |
| OBS-004-3 RecordTable live pill, service chip, truthful footer | `packages/ui/src/lib/components/RecordTable.svelte`; `LIST_LIMIT` exported from `observe.ts` | V1: pill button names (reading/opened), chip, "Showing 3 traces", "newest 3 read, older traces not read" at limit | PASS |
| Shared Logs: Wrap lines, bounded paging/page size, Columns | `RecordTable.svelte` (logs bar, pager, `[25, 50, 100]` per page; local state, no new public input) | V1: 51/101 rows, page 2 `aria-current`, per-page reset, Wrap `aria-pressed`, hidden Message column, "newest 120 read · older records not read" | PASS |
| OBS-004-4 TraceWaterfall axis, caption, collapsed child count | `TraceWaterfall.svelte` | V1: caption, ticks `0s 1s 2s 2.84s`, `+1` titled "1 child spans collapsed"; browser: `+1` visible after the class fix below | PASS |
| OBS-004-5 RecordDetail role blocks, counted tabs, exception banner, attribute side | `RecordDetail.svelte` | V1: tabs `Messages 3/Attributes 6/Events 1/Exception/Raw`, role headings `system instructions/user/tool`, side `dl` without message keys, banner `TimeoutError: search timed out`, Raw retained; bare span has no counts or banner; log detail severity tone, body, attributes, Filter `service_name` + Open trace intents | PASS |
| Shared Attribute Filter/Exclude/Open trace | `RecordDetail.svelte` actions | V1 log-detail intent sequence | PASS |
| Shared Metrics Latest/Max/Mean + line visibility | `Chart.svelte` series table, `hidden` set drives drawn lines, bounds, tooltip, aria-label | V1: a 140/140/130, b 80/80/80; untick hides only its polyline; "2 series · 1 shown"; Show all restores strokes | PASS |
| OBS-004-6 authored SQL aliases carry no telemetry meaning | `RecordTable.svelte` (`signal` undefined for SQL: raw headings, plain text cells) | V3: raw headings, no pill/bar/time/chip/error row, exact text; converse with canonical spans; WyrdView path | PASS |
| OBS-004-7 typed group identity | `RecordTable.svelte` groups keyed by `JSON.stringify(value)`, collisions captioned with the typed value | V1: `∅ ("∅")` vs `∅ (null)`; `1ms (1000000)` vs `1ms (1000400)`, 5 groups | PASS |
| Generic semantics: names never drive presentation | inventory below | V4: `status.error`/ok renders ok, HTTP status 2 renders error, rename-invariant tones, status change flips one row, exact facet/open values, series rename keeps strokes, waterfall rename-invariant, HTTP-only span has no synthesized roles, WyrdView path | PASS |
| Real data, authorization, built host | unchanged loaders; V2 owner untouched | V2 | PASS |
| StatStrip conflict recorded, contract unchanged | gallery checkpoint note only; `StatStrip.svelte` unchanged | diff audit | PASS (unresolved, see below) |
| Human visual acceptance per component, desktop/mobile, light/dark | — | not obtained | PENDING |

### Runtime condition inventory

Every host (SvelteKit app routes, `examples/ui-app`, the MCP App
`dist/wyrd-view.html`) renders through `WyrdView` and the shared components.
A search of `src/`, `examples/ui-app/src` and `packages/ui/src/lib` found no
host-local name, substring or attribute-value rules.

| Owner | Condition | Semantic source |
|---|---|---|
| `RecordTable.svelte` | error row / `StatusPill` tone | `status_code` (0 unset, 1 ok, 2 error), traces/spans signal only |
| `RecordTable.svelte` | duration bar, `<time>`, service chip, word headings | canonical column of a signal list; never for `sql` input |
| `FacetRail.svelte` | `error` tone, status words | `status_code` value 2 / `STATUS_NAMES` |
| `TraceWaterfall.svelte` / `observe.ts spanTree` | error span | `status_code === STATUS_ERROR` |
| `RecordDetail.svelte` | error heading/badge | `status_code === 2` (spans) |
| `RecordDetail.svelte` | log severity tone | `severity_number` 13–16 warn, ≥ 17 danger |
| `RecordDetail.svelte` | message blocks and role labels | `gen_ai.system_instructions`, `gen_ai.input.messages`, `gen_ai.output.messages`; role from the recorded `role` field, else the attribute key |
| `RecordDetail.svelte` | exception banner | `exception_type`, `exception_message` columns |
| `observe.ts` stats | `errors` | `status_code = 2` |
| `observe.ts` stats | `invocations` | approved `gen_ai_operation_name = 'invoke_agent'`, StatStrip only |
| `Chart.svelte` | series stroke/marker/dash | series index; never the label |
| `plot.ts` | `other (aggregate)` caption | collision with a literal `other` label only; no styling |

### Agent visual comparison

Source: local Vite dev server for this worktree on port 5211. The
browser-extension Chrome could not reach the sandboxed server (connection
refused), so captures were taken with headless Google Chrome (macOS).

| Component | Board / region | Compared | Disposition |
|---|---|---|---|
| FacetRail | Main, Spans, Logs — left facet column | light 1440, min viewport | hint, five values, `+ N more`, `✓` pressed, error tone match. Board facets show fields this fixture lacks (DURATION buckets, ROOT SPAN, "+ Add a field"): page tasks |
| RecordTable traces/spans | Main trace list, Spans flat list | light 1440, narrow 360 | chip, red error rows, duration bars, status pills, footer match. **Unresolved:** trace-row GenAI sub-line (model/tool/tokens) not synthesized; Spans TRACE, PARENT, TOKENS columns absent |
| RecordTable logs | Logs records list | light 1440 against `Logs.png` | header bar, Wrap lines, Columns, pager, per page match. **Unresolved:** severity text not coloured (list does not read `severity_number`); board TRACE column vs implemented EXCEPTION column |
| Chart (Metrics) | Metrics series summary | light/dark 1440, narrow | Latest/Max/Mean table, visibility. **Fixed here:** host `th` styles uppercased series names and Tailwind preflight stacked the swatch above them. Narrow legend scrolls horizontally. **Unresolved:** ServiceCharts compact legend now shows the series table |
| TraceWaterfall | TraceDrawer upper tree/timeline | light and dark 1440 against `TraceDrawer.png`, narrow | caption, axis, guides, selected row, red error path, `+N` match. **Fixed here:** `+N` used class `hidden`, which the host's Tailwind `hidden` utility set to `display: none` (jsdom cannot see this). Renamed `collapsed` |
| RecordDetail | TraceDrawer lower detail; Logs opened record | light 1440, narrow | header badge, meta, banner, counted tabs, role blocks, side attributes, actions match; on narrow the side region stacks below |
| StatStrip | Agent, Service, Home summaries | light 1440, narrow | unchanged; conflict below |

Theme and viewport: populated dark compared for TraceWaterfall and Chart;
other dark checkpoints were rendered but not individually compared. Headless
Chrome has a 500 px minimum viewport. At that width the page has no horizontal
overflow (`scrollWidth` 500 = viewport), and wide tables scroll inside their
own regions. The gallery's 360 px `narrow` cases cover the narrower layout.

Each checkpoint links its board PNG through a labelled region rather than a
stored per-crop comparison image.

### Unresolved, not accepted

- **StatStrip presentation conflict:** the inventory says numbers with
  sparklines, while the Agent, Service and Home boards differ. The published
  contract is unchanged; this needs a human decision (task audit, StatStrip).
- The discrepancies marked **Unresolved** above stay unaccepted until a human
  approves an exception or a page task adds the region. The WyrdView-level
  Columns/CSV/shell controls belong to the page tasks.

### Commands

| Command | Result |
|---|---|
| V1 `… test src/lib/ui-library.test.ts -t '^TASK-004: Canonical signal interactions$'` | 1 passed |
| V2 `with-test-postgres.sh … -t "^TASK-004: Authorized Observe across hosts$"` | 1 passed. Run before the final CSS-only fixes (class rename, legend `th` style), which no V2 assertion reads |
| V3 `… -t '^TASK-004: Authored query aliases do not declare telemetry semantics$'` | 1 passed |
| V4 `… -t '^TASK-004: Telemetry presentation is independent of record names$'` | 1 passed |
| `pnpm test` (whole UI package) | 44/44 passed, 8 files |
| `pnpm check` | 0 errors, 0 warnings in both projects |
| `pnpm build` | done |
| `mise run codegen:check` | in sync; no generated drift |
| `git diff --check` | clean |

### Diagnoses

- `pnpm check` reported "Cannot use 'state' as a store" in FacetRail. The local
  `state` shadowed the `$state` rune. Renamed it `filterState`.
- V1 failed on the old live-button name `↑ 1 new traces`. The name now
  includes the hold reason by design, so the assertion matches the new name.
- New V1 assertions first failed for test-selector reasons: an empty
  `definition` role query, and the `EmptyState` note matching
  `queryByRole('note')`. Both now target the component's own labelled element.
  No production change was involved.

### Scope

Non-goals respected: no new public input or pagination/filter contract, no
StatStrip contract change, no page composition, no loader/query change except
exporting `LIST_LIMIT`, and no generated artifact edited by hand. Files
changed: the five Observe components, `observe.ts`, `ui-fixtures.ts`,
`ui-library.test.ts`, the gallery page and the new dev-only board route. Other
modified `changes/` files predate this session and were left untouched.
