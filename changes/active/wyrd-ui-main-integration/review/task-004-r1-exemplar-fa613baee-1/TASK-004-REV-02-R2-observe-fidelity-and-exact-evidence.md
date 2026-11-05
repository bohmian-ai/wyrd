---
id: TASK-004-REV-02-R2
kind: remediation
status: proposed
spec: SPEC-wyrd-ui-main-integration
spec_revision: 18
parent_task: TASK-004-REV-02
remediates: [FIND-TASK-004-REV-02-1, FIND-TASK-004-REV-02-2, FIND-TASK-004-REV-02-3, FIND-TASK-004-REV-02-11]
---

# Finish Observe fidelity and exact evidence

The R1 candidate now has the conventional server loads, reusable TimePicker,
left rail and shared signal components. Keep that work. It still clips required
controls, hides metric summaries, uses the wrong Logs/drawer composition, and
fails several exact-metric and held-row cases. Complete those existing outcomes
and remove the committed candidate captures. A disclosed difference is not an
approved exception to the agreed boards.

## Subject and execution authority

Execute through `$wyrd-implement` with `$wyrd-ui`, using this task together with
the original assignment and R1. This is correction within approved revision 18,
not permission to redesign the UI or introduce a new data/wire contract.

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Approved spec: `changes/active/wyrd-ui-main-integration/spec.md`, revision 18.
- Original task: `changes/active/wyrd-ui-main-integration/tasks/TASK-004-revision/TASK-02-live-observe-workflow.md`.
- Prior remediation and evidence: `changes/active/wyrd-ui-main-integration/review/task-004-rev-02-f6aeb83a7-1/TASK-004-REV-02-R1-approved-observe-ui.md`.
- Original base: `4a76331113864f9bddc43d3cc8df01080a696937`.
- Initial reviewed candidate: `f6aeb83a77179ac4450110671df6f564adc1914a`.
- R1 rejected candidate / next review's prior candidate: `fa613baee5e70f9458442f22d74ecebc22893ebc`.
- This review: adjacent `verdict.md`, `findings-validation.md`, `task-review-remediation.md`, `maintainer-review.md`, `followup-review.md`.
- Design: existing `references/canvas/{Main,Spans,TraceDrawer,TimePicker,Logs,Metrics}.dc.html` and PNGs under this change; current brand and schema authority override illustrative values only.

Read the entire original task, R1 and this packet before editing. UI-relative
paths below begin at `crates/wyrd/wyrd-server/wyrd-ui/`. CodeGraph and repository
rules still apply. Review source stays immutable; only subsequent implementation
executes this packet. F5–F10 are closed by this review's scoped evidence; do not
reimplement or rerun unrelated coverage without an affected path.

## F1 — Finish the actual board compositions and usable controls

**Diagnosis.** `src/lib/TimePicker.svelte:215–227` anchors its wide panel to a
wrapped trigger rather than the viewport. At 390px the left side of the open
picker is clipped. It also adds a literal decorative shadow contrary to the
brand rule. The Chart legend's unbounded raw-JSON identity heading displaces
Latest/Max/Mean offscreen in the Metrics desktop capture (`Chart.svelte:394–404,
606–638`; captions from `plot.ts::recordedSeries`). Logs mounts a full generic
Chart at Observe `+layout.svelte:254–258`, adding a large legend/footer instead
of the compact severity timeline. The trace page uses a generic 480px Drawer
and title `Trace` (`+page.svelte:35` and shared Drawer), where the approved board
uses a wide trace header/waterfall/detail surface. Columns/CSV/Wrap remain in a
second table header rather than the agreed listbar. These are observable control
and investigative-content failures under F1, not harmless illustrative data.
Recorded tests do not establish their viewport/board closure.

**Selected correction.** Finish composition through existing owners:

- Keep one app TimePicker for every signal, but constrain its open panel to the actual available viewport/container after toolbar wrapping, including visible From/To, Relative/UTC and action controls at 390px. Keep keyboard/focus/touch behavior and recent/validation semantics; remove the decorative shadow and use current brand tokens.
- Preserve the typed recorded metric identity for equality and selection. Give shared Chart's metric summary a concise bounded human caption and a keyboard/touch-accessible way to inspect full recorded dimensions. Keep the exact Latest/Max/Mean values visible/scannable in the board region instead of requiring scrolling past a whole JSON identity. Do not truncate the underlying evidence or replace identity with the caption.
- Use the existing shared compact Chart presentation for Logs' severity-stacked timeline and severity key. Preserve its Open in Query action and exact executed SQL/binds through the established action mechanism; do not remove generic Chart legends/footers for other consumers.
- Extend the existing Drawer presentation boundary to support the TraceDrawer board's useful desktop width (approximately 884px within the 1440px reference), trace identity/summary and Link/Expand/close controls, upper waterfall and lower details. Keep other Drawer callers, modal focus/Escape/close behavior and real mobile viewport coverage intact. Do not replace TraceWaterfall or RecordDetail.
- Expose a presentation seam from the existing RecordTable control owner so Observe can place Columns/CSV/Wrap in the approved listbar. Keep behavior and export/selection state single-owned; do not reimplement it in the page or render duplicate controls.

Each correction reuses an existing owner; a new kit, table, modal, query service
or arbitrary replacement design is unnecessary. Merely listing these deviations
or taking screenshots cannot waive the board requirement.

**Scope calibration.** Unsupported p50/p90/p99 must stay unavailable: original
REQ-039 and R1 forbid fabricated percentiles. Preserve that truthful constraint
rather than enabling fake results to copy illustrative values. Current local
brand, schema/identity fields and real data also remain authoritative. A fixed
dialog's position in a stitched full-page screenshot alone is not a live-mobile
bug; verify its real viewport instead. These distinctions do not excuse the
actual clipping, hidden summary, misplaced controls or wrong board composition.

## F2 — Keep a held table's projection and rows coherent

**Diagnosis.** Conventional SSR and common range/URL ownership are corrected.
The R1 `oncolumns` extension exposes a new seam: RecordTable keeps an old `held`
result while deriving headings from the new `input.columns` (`:48–69,152–182`).
A live list held after opening a record can add a previously unread column;
new props arrive, but cells/CSV read that column from old rows using index -1.
The new heading then shows missing data although the successful new read contains
it. This violates F2's preserved investigation state and truthful evidence.

**Selected correction.** Correct the transition once in the shared RecordTable
held-snapshot owner. An explicit changed column projection must commit its
matching input and newly read result coherently when available, instead of
applying new headings to incompatible held rows. Keep ordinary same-projection
Live refreshes held, with existing row/count/new-row/resume behavior. CSV and
Open in Query must describe the displayed snapshot. Do not patch page cells,
silently blank absent indices, remount the entire table on every live read, or
disable holding. This is a bounded snapshot-transition correction, not a new
cache or per-page table owner.

## F3 — Correct exact metric comparison and retained-point rendering

**Diagnosis.** Typed dimensions and no-fold metric shaping are substantial
corrections, but two downstream assumptions still fail:

1. `observe.ts::metricQueries:388–405` chooses exact Max by a rounded DOUBLE sort key and then an integer tie-break. Integer -9007199254740993 and double -9007199254740992 collapse to the same double key, so the smaller integer can be reported as maximum. An exact output string does not make its selection exact.
2. `plot.ts::recordedSeries:83–105` correctly retains equal-time samples, but `Chart.svelte:345` keys markers by `point.x`. Two valid retained samples then collide and Svelte throws `each_key_duplicate`. Existing fixtures use distinct instants and miss this consumer seam; the reviewer reproduced it with the installed key validator.

**Selected correction.** Fix Max selection at the existing shared Metrics
SQL/projection owner using exact comparison for supported integer/double samples;
do not use approximate chart geometry as the summary's numeric authority or
change which representation always wins a rounded tie. Return the actual winning
recorded value as exact text and preserve latest-by-time semantics, explicit
aggregation and typed identities. Keep labeled geometric approximation separate.
At shared Chart, identify each preserved point occurrence distinctly when
rendering markers, retaining every sample and its timestamp. Tooltip/inspection
must expose retained equal-time occurrences truthfully rather than silently
claiming the first is the only sample. Do not deduplicate,
sum, discard or shift points to satisfy render keys. Keep generic authored-query
shaping unchanged. These are local producer-selection/consumer-render corrections,
not a new metric engine or JSON format.

## User-approved scope amendment — exemplar precision deferred

On 2026-11-02 the user explicitly removed the remaining unsafe exemplar-integer
precision work from this remediation. FIND-TASK-004-REV-02-4 is deferred, not
fixed, and is not an implementation, proof or acceptance blocker for this
remediation or its current delivery. This direction supersedes the earlier
original-task/R1/R2 requirement to correct that specific remaining limitation.

Exemplar integers outside JavaScript's safe integer range may still round in
the UI. Preserve the readable exemplar structure and already-correct scalar,
histogram and exponential bucket projections. Do not investigate or change
DataFusion, add exemplar reconstruction, change Bifrost's integer result types,
or introduce a new wire encoding for this deferred issue. F3's separate exact
summary and duplicate-point rendering corrections remain in scope.

## F11 — Remove committed candidate screenshots

**Diagnosis.** R1 committed 32 PNGs under
`changes/active/wyrd-ui-main-integration/review/task-004-rev-02-f6aeb83a7-1/candidate/`.
The review skill explicitly limits review directories to reports, and the user
expressly rejects committing candidate screenshots. Visual proof did not
authorize these generated artifacts.

**Selected correction.** Remove those newly committed candidate PNGs from the
tracked repository in the remediation. Preserve the approved
`references/canvas/png/` design assets and prior text reports. Replace tracked
candidate-path evidence references with accurate text describing the reviewed
states and externally available scratch/user review artifacts where available.
Future generated captures stay outside tracked repository/review artifacts.
Do not rewrite Git history, add an archival framework or remove approved
references. Static scoped diff inspection is sufficient proof.

## Required proof and preserved acceptance

| Finding | Direct closure evidence |
|---|---|
| F1 | Actual six-board comparisons at the reference desktop viewport and 390px, both themes, using populated long-identity metrics and trace/log data. Assert open picker/control bounds, readable identity plus visible exact summary values, compact Logs region, trace drawer width/header/actions and listbar controls. Exercise keyboard/focus, touch and relevant loading/empty/denied/error/partial/overflow states. Keep captures outside the tracked repository; record text results and individual human acceptance separately. |
| F2 | Existing component interaction test holds a populated live snapshot, adds a previously unread nonempty column through host props, and proves headings/cells/CSV/Query describe one projection. An ordinary same-schema appended refresh still holds counts/rows and resumes correctly. |
| F3 | Existing real Metrics SQL journey asserts unsafe negative mixed samples yield max -9007199254740992, with positive adjacent/range cases preventing a representation-only tie-break. Existing component test actually renders same-series/equal-time samples without duplicate-key error, retaining all points; keep generic Chart and >20-series/no-fold tests green. |
| F11 | Scoped diff confirms exactly the generated candidate image artifacts are removed, reference design assets/reports remain and evidence text is accurate. No runtime test for artifact removal. |

Use existing Vitest/Svelte component and production-BFF browser owners. Extend
existing named tests in place and confirm their selectors before RED. Record
the failing scenario, correction and GREEN result; static artifact/docs changes
need no manufactured RED. Do not weaken checks or call unselected tests PASS.

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.test.ts -t '^observe signal planning and interactions$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-004: Canonical signal interactions$'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^authenticated (traces and spans|logs investigation|metrics investigation)$"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^TASK-004: Authorized Observe across hosts$"'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
```

Reuse original task's real-server/TS-addon/production-BFF/Chromium prerequisites;
no new runner or alternate environment. Regenerate catalog artifacts through the
existing generator and run `mise run codegen:check` only when its inputs change.
Run `git diff --check` and the scoped consumer audit. Keep any newly changed
Rust items documented and run their owning format/lint/focused checks if a
concrete existing-server fix enters the write set; a speculative server rewrite
is excluded. Do not rerun F5–F10 unrelated lifecycle/SDK journeys, the full SDK
or user-journey suites, broad aggregates, `gate` or the 3,000-request dev replay
for this remediation; those belong to integrated change review unless an actual
new affected boundary requires its focused check.

Preserve the corrected conventional SSR and session Node executor, one applied
URL/range/predicate generation, exact authorized trace/log evidence, attribution,
query inspector, scoped permissions, canonical batch identity/source/wheel and
app-first cleanup, mandatory stub/docs/instrumentation fixes and supported
platforms. No new product destination, Ask, provider, credential/lifecycle owner,
wire encoding, dependency or compatibility layer. Do not reset the whole task.

Append implementation evidence to this remediation without changing its reviewed
identity or previous reports. Next review covers F1–F3/F11 closure and affected
consumers, carrying F5–F10 evidence forward. The required per-board human visual
acceptance is still pending; implementation/test output cannot grant it. Do not
commit or push unless the user asks.

## Implementation evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| F1 TimePicker inside 390px | `src/lib/TimePicker.svelte` clamps the dialog's right offset to the viewport with a 16px gutter | `ui-pages.integration` traces journey: 390px dialog bounds within `[16, 374]` | PASS |
| F1 metric summaries visible with long identities | `Chart.svelte` legend caption truncates (`th:nth-child(2) max-width: 0`), opens a dimensions list on click; Latest/Max/Mean stay visible | `ui-library.test` caption inspection; metrics journey; desktop and 390 captures | PASS |
| F1 Logs compact severity timeline | `observe/+layout.svelte` renders `Chart compact bars` with the severity key and one Open in Query | logs journey: Key info/warn/error, one Open in Query, no legend table | PASS |
| F1 trace drawer composition | `Drawer.svelte` `width`/`heading`/`tools`; `observe/+page.svelte` trace header (root, status, service, id, start, duration, spans, errors), Link, Expand/Collapse, Esc, span divider | traces journey: header text, 884px width, Expand fills viewport, Collapse, Link | PASS |
| F1 Columns/CSV/Wrap in the listbar | `RecordTable.svelte` `controls` target places tools in the layout's listbar | traces journey listbar assertions | PASS |
| F2 held rows keep their projection | `RecordTable.svelte` holds `{data, columns, executed}` together; switches only once loaded rows carry the new projection | RED: held rows rendered new columns before; GREEN: `ui-library.test` held-projection case (old projection while loading; new column, CSV and Open in Query `second` after) | PASS |
| F3 mixed int/double Max exact | `observe.ts` Max orders by double, breaks rounded ties by exact integer | `ui-pages.test` SQL pin; metrics journey `ui.low`/`ui.high` Max | PASS |
| F3 equal-time points render | `Chart.svelte` keys markers by occurrence; tooltip lists every point at the instant | RED: duplicate-key render error before; GREEN: `ui-library.test` twin render (2 markers, tooltip `1, 3`) | PASS |
| F4 exemplar precision | Removed from this remediation by the user; no change | n/a | REMOVED |
| F11 no committed candidates | 32 PNGs under `task-004-rev-02-f6aeb83a7-1/candidate/` removed; R1 report states captures were kept outside the repository | `git status` | PASS |

Commands (all passed): `pnpm test src/lib/ui-library.test.ts src/lib/ui-pages.test.ts` (15);
the three `ui-pages.integration` Observe journeys (3); `ui-library.integration`
`TASK-004: Authorized Observe across hosts` (1); `pnpm check` (0 errors);
`pnpm build`; `node packages/ui/scripts/catalog.ts --check`; `git diff --check`.

Non-goals held: no server, Bifrost, wire, schema or dependency change; UI SQL text
only. Captures were taken outside the repository. Per-board human visual acceptance
remains pending.
