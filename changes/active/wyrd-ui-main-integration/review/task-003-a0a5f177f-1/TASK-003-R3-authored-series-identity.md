---
id: TASK-003-R3
kind: remediation
status: ready
spec: SPEC-wyrd-ui-main-integration
spec_revision: 12
parent_task: TASK-003
remediates: [FIND-003-10]
---

# Preserve authored series identity before aggregation

Execute with `$wyrd-implement` under approved revision 12.
Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
Specification `changes/active/wyrd-ui-main-integration/spec.md`, revision 12 approved.
Original task `changes/active/wyrd-ui-main-integration/tasks/TASK-003-shared-ui-foundation.md`.
Original base `695ca914df61a8683059f6f32279ec2148424e06`; reviewed cumulative candidate `a0a5f177f23960822222b851716460dec02804b3`.
Prior candidates/ledgers/R1/R2: `../task-003-ab1d2499f-1/` and `../task-003-c2bbd8e47-1/`.
Current verdict and independent complete diagnosis: `verdict.md` and `findings-validation.md` beside this task.
Requirements REQ-025/030: one or more numeric y columns plus optional text series must preserve truthful distinct measurements, folding only above twenty.

UI-relative paths below begin at `crates/wyrd/wyrd-server/wyrd-ui`.

## FIND-003-10 diagnosis

`packages/ui/src/lib/plot.ts:36-45` uses a formatted caption as Map identity before summing. With multiple y bindings, concatenating raw group text, a space and column name is not injective. With nullable text, String(null) also equals literal text null. The shared normalized result legitimately preserves nulls, arbitrary UTF-8 strings and arbitrary SQL aliases; ChartInputSchema and shapeChart accept these inputs without a reserved string/delimiter restriction.

At one timestamp, y is `["tokens", "input tokens"]`, series is model, with rows:

| model | tokens | input tokens |
|---|---:|---:|
| model input | 1 | 10 |
| model | 2 | 20 |

There must be four groups with values 1,10,2,20. Both (model input,tokens) and (model,input tokens) become model input tokens, so the candidate produces three lines and a false value 21. With one y, groups actual null=1 and literal text null=2 produce one false sum 3 instead of two groups. Independent accepted-input/source-aligned probes reproduced both. This happens before overflow exists, so the repaired aggregate caption cannot recover the lost measurements. Current 5/20/21 and other-caption cases omit these inputs.

Chart is the only production caller of timeSeries; its plot, tooltip, legend and accessible name all consume the corrupted result. Direct consumers, WyrdView using Node/MCP/remote loaders, gallery/custom app and bundled AppBridge converge here. No consumer can recover the tuples. barRows maps positions without this grouping and is not another correction owner. columnIndex, result normalization, loaders, SDK/MCP and server auth remain correct existing owners.

## Selected minimal correction

Keep the correction in existing timeSeries. Before summation, use conventional JSON serialization of the typed `[raw series value, selected y binding]` pair as the private Map key when series is bound; keep existing y-only identity without a series binding. JSON typing/escaping preserves null versus string and arbitrary delimiters. Retain source tuple information for caption attribution rather than using formatted text as identity.

At the same producer, derive distinguishable captions for returned authored groups, then choose the recognizable synthetic other caption against that final caption namespace. Preserve ordinary unambiguous captions. Qualify ambiguous authored captions with escaped underlying group/metric information so tooltip/accessibility values remain attributable, including null versus text null; ensure uniqueness even if arbitrary authored strings match qualification text. Reuse existing deterministic collision qualification; exact local spelling/suffix mechanics remain implementation-owned. Unique numeric suffixes alone without source attribution do not identify which tuple owns a value.

Preserve existing private Series label/points return shape and all Chart consumers. Preserve first-match binding, same-tuple/same-instant summing, repeated identical binding semantics, null-y omission, temporal/numeric encodings, ordinary order, deterministic total ranking, threshold twenty, nineteen-plus-one overflow and exact per-instant fold totals. Retain FIND-003-9's collision-safe aggregate caption and all earlier fixes. No new public identity field, aggregate flag, schema restriction, reserved name, dependency, plotting framework, host workaround or transport change is needed.

Changing a delimiter or null sentinel cannot solve the full accepted domain. Do not reject/rename query aliases, hide a line, merge distinct groups intentionally, remove renderer keys alone or add per-consumer guards. Fixing identity at the producer prevents invalid measurements and makes all sibling output truthful at once.

## Acceptance and focused proof

Use the existing portable-rendering scenario and mount/focus/rerender facilities, with scenario-by-scenario Red/Green:

- The accepted two-y/two-group example renders four separately attributable plot and legend entries with values 1/10/2/20 in tooltip and accessible captions; no false 21 line.
- Actual null versus text null renders two separately attributable values 1 and 2, without false sum 3. Include qualification-like strings and escaping cases that would defeat a fixed alternate caption, preserving full typed pair identity.
- Changed ranking/cardinality on rerender shows current labels/values. Existing same-group summation, duplicate-column, null-y, signed geometry, 5/20/21, authored other/qualification and fold-total regressions stay green.

Do not add a new harness or live server journey for this pure producer-to-component behavior. Private helper and fixture structure remain implementation-owned. Run:

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Validated portable rendering$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise exec -- git diff --check
```

The existing test script retains native public-import smoke automatically. A pure producer assertion may supplement but not replace mounted attribution proof. No generated input changes are needed; do not prescribe codegen, Rust/release/Linux aggregates or broad journey suites for this write set. Record exact RED/GREEN/check results and limits. Preserve separate TASK-027 changes, all closed FIND-003-1..9, server-owned behavior and non-goals (no Ask/A2UI/persistence/MCP registration/npm publication). Stop for any genuinely material public/product/durable decision; this correction requires none. Next review reassesses the complete cumulative original task and all remediation rounds.

## Implementation evidence

Status `IMPLEMENTED` on the post-#205 rebase (`ff895d159` base; pre-rebase SHAs above describe unchanged code).

Owner and gap: `timeSeries` in `packages/ui/src/lib/plot.ts` is the only producer; it keyed groups by formatted caption. It now keys groups by `JSON.stringify([raw series value, y])` when series is bound, keeps the y-only key otherwise, and keeps the tuple beside the points. A private `captions` helper keeps ordinary captions. Captions that clash become their group's JSON spelling (value, plus `y` when several y are bound), repeating until none clash. JSON is injective per tuple, so qualified captions never collide with each other, and the loop ends because each pass qualifies at least one new group. The "other" aggregate caption is then chosen against the final authored captions with the existing FIND-003-9 loop. The `Series` shape and every Chart consumer are unchanged.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Two-y/two-group example renders four attributable entries 1/10/2/20, no false 21 | `plot.ts` `timeSeries` typed pair key; `captions` | portable-rendering scenario: legend `['"model input","tokens"', 'model input input tokens', 'model tokens', '"model","input tokens"']`, 4 polylines, aria-label, tooltip rows | PASS |
| Actual null vs text null render separately as 1 and 2; qualification-like/escaped authored text stays distinct | `captions` fixpoint over JSON spelling | scenario: `[null,1],['null',2],['"null"',4]` gives captions `null`, `"null"`, `"\"null\""` with matching tooltip values | PASS |
| Rerender with changed cardinality shows current labels and values | unchanged Chart derivation over new producer output | scenario rerender: `['null','x']`, `null: 5 ms`, `x: 6 ms` | PASS |
| Existing summation, duplicate-column, null-y, signed geometry, 5/20/21, authored other/qualification, fold totals stay green | ranking/fold/aggregate caption logic preserved | full `TASK-003: Validated portable rendering` scenario | PASS |

Commands (from repository root):

- RED, before the `plot.ts` change: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Validated portable rendering$'` failed. Received legend `['model input tokens', 'model input input tokens', 'model tokens']` (three lines, merged group).
- GREEN: same command, 1 passed, 9 skipped.
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build`: pass.
- `git diff --check`: pass.
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check`: fails only on the 6 known flat-identity `space` errors in `packages/ui/src/lib/loaders.ts` and `CardHeader.svelte`. Those are owned by the parallel identity remediation and are outside this write set. Because the package check fails first, the app check was run directly with `svelte-check --tsconfig ./tsconfig.json`. It reports 0 errors in `plot.ts` or the TASK-003 test. Its remaining 3 errors are the same `space` family in `src/lib/ui-fixtures.ts` and `src/lib/ui-library.integration.test.ts`, also outside this write set.

Limits: in a fresh worktree, `examples/ui-app/.svelte-kit/tsconfig.json` has to be generated with `svelte-kit sync` before the vitest file transforms, because the test imports the example route. This is a setup prerequisite and was not changed here. Non-goals stayed excluded: no schema, transport, host, dependency or generated-artifact changes, and only `plot.ts` plus the TASK-003 portable-rendering scenario changed.
