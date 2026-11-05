# TASK-003 independent structured Ponytail validation

One material finding is retained: **FIND-003-10**. The diagnosed causes of FIND-003-1 through FIND-003-9 are closed in current source. This is a validated finding ledger and correction recommendation, not the task verdict.

## Subject and independent method

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; original base `695ca914df61a8683059f6f32279ec2148424e06`; immutable candidate `a0a5f177f23960822222b851716460dec02804b3`. HEAD independently matches the candidate. Tracked source is unchanged; the only write by this reviewer is this report. No CodeGraph directory exists.

Read this attempt's subject, verification, comparison, both implementation reviews, standards, maintainer, system and security reports. No follow-up exists; the comparison records why none was triggered. Read both historical verdicts/validated ledgers and the R1/R2 tasks with their execution evidence. Discovery agreement and historical closure statements were hypotheses, not proof. The full cumulative task remains the subject; the R2 diff only locates corrections. Separately approved TASK-027 and the subject's excluded/upstream commits are not foundation drift.

Applied the original TASK-003, approved specification revision 12 (especially REQ-025/030/033 and INV-001–007), AGENTS.md, agent-rules, reference router, spec-driven-development, maintainer and TypeScript/error guidance, Wyrd design's doctrine/client ownership, doctrine summary, and repo UI boundary guidance. Inspected the current complete grouping/shaping/schema/rendering owners and their actual callers, rather than treating a green build as semantic proof. Expanded through Node/MCP adapters, the shared positional JSON producer, SDK collection/cancellation, WyrdView loading/action lifecycle, both endpoints, direct/gallery/example/AppBridge hosts, package exports/generator, shared-client query-only change and production bundle closure.

UI-relative paths below begin at `crates/wyrd/wyrd-server/wyrd-ui/`.

## Source-backed prior closure

| Stable ID | Independently inspected source and sibling consumers | Disposition and proof scope |
|---|---|---|
| FIND-003-1 | `loaders.ts:156–162` selects explicit `input.range` before inherited context; actual request and `executed` reuse the same params. WyrdView, both endpoint hosts and direct example SSR converge on this loader. | CLOSED. Existing two-panel brush/reset and clock-pinned relative override assertions address the precedence cause. |
| FIND-003-2 | `shapeChart.need` uses `columnIndex` first-match selection, as do `timeSeries`, `barRows` and explicit stat extraction. Table loops retain columns/cells positionally. | CLOSED. Both conflicting descriptor orders, x/series roles and positional-table assertions remain. No duplicate renaming or rejection was introduced. |
| FIND-003-3 | `Chart.svelte:46–56,123–163` derives signed domains around zero and bar endpoints with nonnegative heights; negative labels have reserved space. All component hosts use the same geometry. | CLOSED for the signed-domain diagnosis. Existing mounted mixed/all-negative/flat-negative/zero/positive cases and supplied browser evidence cover that correction. |
| FIND-003-4 | `plot.ts:15,48–60` folds only above twenty; Chart's four markers and five dashes give twenty unique pairs in plot and legend. | CLOSED for premature threshold/style indexing. Existing 5/20/21 counts, styles and sum assertions stand. The new identity loss occurs before counting and is separately diagnosed below. |
| FIND-003-5 | CardHeader reads named version/tab states, disables pending/unavailable controls, resets selection to authoritative input and renders shared ActionNote; WyrdView validates and returns host outcomes through panel.actions. | CLOSED. Direct controls and WyrdView-delivered refusal tests address discarded outcomes. These consumer guards belong at controls also used directly. |
| FIND-003-6 | `node.ts:30–43` closes an already-aborted acquired stream before collect, attaches/removes the active listener and rejects post-collect cancellation. SDK `BifrostQueryStream.return` calls native.close; endpoints forward request.signal; WyrdView additionally fences obsolete generations. | CLOSED. Existing acquisition/active/pre-abort assertions cover the original timing windows. Other reads have no supported stream-close capability to duplicate. |
| FIND-003-7 | Catalog generator copies Rust-generated problem examples; `contract.ts::wyrdProblem` and local loader/view/MCP/WyrdView producers reuse them. Both endpoint refusal functions serialize the full canonical object. Upstream code/title and denied mapping remain. | CLOSED for uncatalogued identity/reduced HTTP projection. Canonical assertions cover both endpoints and the browser/native boundary. No new HTTP internal-exception disclosure was added. |
| FIND-003-8 | `contract.ts:19` has the standard JSON import attribute; public loaders/MCP share that import. Existing root test script builds then runs native imports plus canonical validation before transformed Vitest. | CLOSED. Current source/emitted package and root's fresh native smoke establish linking. This validator's native grouping probe also successfully imported public loaders. |
| FIND-003-9 | `timeSeries:55–59` chooses synthetic other/aggregate/suffixed caption only when absent from all kept captions. Chart plot/legend/tooltip keys and accessible text share that output. | CLOSED for authored-versus-synthetic overflow collision. R2 tests retain real other and qualification text, exact totals, unique twenty captions and rerendered ranking/cardinality. |

These closures do not imply arbitrary authored group identity is sound. R2 fixes the overflow label after grouping; it cannot recover values already merged during grouping.

## Proposal reconciliation and stable IDs

| Discovery claim/correction | Independent disposition |
|---|---|
| B-003-10: different `(series,y)` tuples merge before overflow | CONFIRMED from current schema, producer and executable reproduction; FIND-003-10. |
| INV-003-10: same tuple merge plus nullable null versus literal text null | CONFIRMED; both manifestations share the same coercion/caption-as-key cause and belong to FIND-003-10. |
| MNT-003-10: caption identity aliases multi-y groups | CONFIRMED; duplicate of FIND-003-10, no second finding. |
| All three recommend one producer correction with private typed identity and distinct captions | REVISED into the selected complete approach below. A change of delimiter or a fixed null caption is insufficient across the accepted domain. |
| Standards/system/security reports propose no material findings | No additional retained finding; their scoped results do not establish plotting correctness or waive missing execution evidence. |

**FIND-003-10 is new, not a reopened FIND-003-4 or FIND-003-9.** The former diagnosed a cap of four and unsafe style access; those mechanisms now satisfy twenty. The latter diagnosed a synthetic label colliding with a retained authored label after overflow; its exact correction is present. Here two authored identities merge at `plot.ts:42–44` with only four logical groups (or two nullable groups), before overflow exists. It predates R2 in task-owned cumulative source. The diagnoses require distinct input and correction boundaries even though they share a broader history of treating captions as identity. Preserve 1–9 as closed; consolidate both new manifestations into the next unused ID, 10.

## Final deduplicated ledger

### FIND-003-10 — authored series identity is lost before aggregation

**Status:** CONFIRMED; correction recommendation refined as below. **Discovery sources:** B-003-10, INV-003-10, MNT-003-10. **Classification:** INCORRECT.

**Violated obligation:** REQ-025 permits one or more numeric y columns and an optional text series, with folding only above twenty; REQ-030 requires complete truthful Chart rendering over preserved positional values. The existing owner's own contract promises one line per series-value × y binding. No reserved series strings, alias characters or non-null restriction exists.

**Exact source:** `packages/ui/src/lib/plot.ts:36–45`, especially caption construction at line 42 and Map lookup/sum at lines 43–45. Reachable admission is `schemas.ts:67–85` and `loaders.ts:124–151`; downstream consumers are `Chart.svelte:43,209,233,256–270`.

**Full accepted producer domain and source of invalid state.** ChartInputSchema accepts any nonempty column name, including whitespace, quotes, punctuation and qualification-like text. Timeseries y contains one or more names; series is optional. shapeChart resolves the first matching descriptor and admits Timestamp/Date x, numeric Int/UInt/Float/Decimal y and Utf8/LargeUtf8/Utf8View series. Nullability is retained and not refused. The real shared `crates/shared/wyrd-queue/src/json_result.rs::JsonResultBuilder::accept_schema/accept_batch` preserves ordered descriptors and emits actual null slots as JSON null, otherwise using Arrow JSON values. Both server MCP collection and SDK native collection use this owner; SDK collect parses the complete projected JSON, and the adapters pass it through. Null versus text "null" is therefore a real permitted value distinction, not a malformed fixture or a JS-only undefined value.

At timeSeries, x becomes epoch milliseconds; invalid parsed x and null y are skipped as before. Without a series binding, the selected y name identifies its group. With a series binding, identity requires the actual string-or-null series value and the selected y binding. Repeated rows in the same tuple at the same instant legitimately sum; different tuples must remain separate. Duplicate descriptors still bind the first position, and repeated authored y declarations do not require new semantics. Empty text, Unicode, delimiters and null-like strings are all in the accepted string domain; do not normalize their identity. Numeric string encoding and temporal parsing are adjacent behavior to preserve, not part of this correction.

The existing producer instead uses a concatenated display string as its Map key. For multi-y, distinct pairs can concatenate identically; for nullable series, `String(null)` equals `String("null")`. `points.set` adds the measurements into the same Map immediately. Ranking and overflow then see an already-reduced cardinality.

**Independent executable evidence.** Ran a read-only `mise exec -- node --input-type=module` heredoc from repository root against existing dist/plot.js and dist/loaders.js. Checked the current source's same grouping expression, ChartInputSchema acceptance and shapeChart populated outcome before asserting the erroneous result. No source/test/harness/build was changed. The approved result can be produced by ordinary quoted SQL aliases, with range binds retained in the accepted Chart input.

At `2026-10-20T00:00:00Z`, y is `["tokens", "input tokens"]`, and series is model:

| Raw series value | tokens | input tokens |
|---|---:|---:|
| `model input` | 1 | 10 |
| `model` | 2 | 20 |

Four distinct pairs should have values 1,10,2,20. Actual output has only three:

```json
[{"label":"model input tokens","points":[{"x":1792454400000,"y":21}]},{"label":"model input input tokens","points":[{"x":1792454400000,"y":10}]},{"label":"model tokens","points":[{"x":1792454400000,"y":2}]}]
```

The same accepted nullable Utf8 result, with y `["tokens"]`, raw groups null and `"null"`, and values 1 and 2, produces:

```json
[{"label":"null","points":[{"x":1792454400000,"y":3}]}]
```

The probe exits 0 because it asserts these reproduced failures. The visible false sums are 21 and 3; green existing tests do not falsify them.

**Writers, callers and sibling consumers.** timeSeries is the sole production grouping/ranking/folding writer and is called only by Chart's timeseries derivation. Chart consumes its output in polylines/markers, the keyed legend, keyed tooltip and accessible plot name. They cannot reconstruct lost tuples. Direct package consumers, gallery and example SSR render Chart; WyrdView calls the same Chart after loadPanel using either Node/MCP executors or the remote BFF/custom endpoint. The AppBridge entry uses mountView with that MCP executor. All routes converge on the same faulty producer, with no downstream repair writer. `barRows` is the sibling shaping function: it maps positional rows and never sums groups by caption; its bar/stat/table consumers do not need this identity correction. `columnIndex`, shared JSON normalization, SDK/MCP contracts and loaders remain their existing complete owners. Series is not separately exposed by the package's declared exports, and its current label/points return shape is sufficient.

**Observable consequence:** accepted measured values silently become false summed measurements; a separately measured value vanishes, every visual/assistive consumer repeats the wrong attribution, and subsequent cap/ranking can use the wrong number of groups. This is required task behavior, not optional hardening or a dormant API.

**Selected smallest complete correction.** Keep the correction in the existing timeSeries producer. Use an injective private representation of the typed source pair before any summation—select the conventional JSON serialization of `[raw series value, selected y binding]` as the Map identity, with the existing y-only identity when no series is bound. JSON escaping and null typing cover arbitrary accepted strings and nullable text without a reserved delimiter/sentinel, dependency, new public field or transport change. Keep source tuple information available for caption attribution. Do not feed a formatted caption back into identity.

At that same producer, derive final distinguishable captions for the actual returned authored groups, then choose the recognizable synthetic other caption against those emitted captions. Preserve ordinary unambiguous captions. For collisions, qualify captions with the escaped underlying group/metric information, retaining null versus literal text attribution, and ensure the final namespace is unique even when arbitrary authored strings equal proposed qualification text. Reuse the existing deterministic collision-qualification behavior rather than another normalization pass at each renderer. The spelling/suffix mechanics are private implementation details; the required choice is typed identity before accumulation and globally unambiguous attributable output from the same owner. A mere numeric discriminator with no source attribution does not adequately identify which group owns a tooltip value.

Preserve same-tuple/same-instant sum, first-match column binding, null-y omission, temporal/numeric handling, ordinary output ordering, deterministic total ranking, twenty-group threshold, nineteen-plus-one overflow and exact per-instant fold totals. Retain FIND-003-9's collision-safe synthetic caption behavior. Unique labels remain appropriate at the consumers because they are the producer's shared presentation identity and accessible attribution; no extra consumer guard is needed.

Deletion cannot satisfy complete Chart rendering. Existing column selection and JSON/result/renderer owners do not supply grouping identity. The standard JSON operation plus the existing Map/caption mechanism solves it without new machinery. Reject replacing one separator, using a fixed null sentinel, rejecting valid text/null results, altering SQL aliases, merging distinct groups, hiding a line, removing renderer keys alone, adding an ID/aggregate flag to the public schema, or introducing a second plot framework. These either leave the false sum/ambiguous text or expand scope. No product, server, credential, persistence or expensive-to-reverse decision is needed.

**Focused closure proof.** Extend the existing `TASK-003: Validated portable rendering` scenario and its existing mount/focus/rerender support:

1. Assert the accepted two-y/two-group example produces four independently attributable plot/legend entries and all 1/10/2/20 values in the focused tooltip and accessible captions; no false 21 line. Preserve source group/metric attribution, not only unique key count.
2. Assert nullable null versus literal `"null"` produces two separately attributable lines/values 1 and 2, not a sum 3. Include qualification-like authored text appropriate to the chosen caption mechanism so a fixed replacement cannot pass; punctuation/escaping must not collapse the underlying pair.
3. Rerender changed groups/cardinality and verify current values/captions. Keep the existing same-group point aggregation, duplicate-column, null-y, signed geometry, 5/20/21, authored other/qualification and fold-total regressions. No new live-server journey is necessary for this pure producer/component defect.

Run the exact affected scenario and existing package checks/build; existing test script also retains the native import smoke:

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Validated portable rendering$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise exec -- git diff --check
```

An optional pure producer assertion can supplement the mounted assertions inside the existing test owner; it must not replace attribution proof. No new harness, dependency, generated contract, Rust aggregate, broad journey rerun or Linux release lane belongs in this bounded remediation. Source changes should reach the private producer and directly affected existing regression assertions only, unless current evidence identifies a necessary adjacent consumer change.

## Evidence reconciliation and limits

Root's verification.md records nine exact focused selectors, UI/example check/build and native public import smoke green. These are supplied fresh execution results, not this validator's reruns; the ten-test file was not freshly rerun by root. Historical full-file, real SDK/server/MCP/BFF/AppBridge, Rust/codegen/native-bundle and human visual approval evidence remains historical/supplied. The new probe independently demonstrates failures absent from those assertions.

The cumulative source still uses the expressly required portable loader/renderer, existing SDK Cards/Bifrost/Execution and MCP owners, shared bounded normalization and Rust-owned selected-session authorization. Query-only client construction reuses the existing sink/assembly shape; no second lifecycle is needed. TASK-027's ResizeObserver.notify extension drives the existing test callback/targets and does not change production cancellation or grouping; that task's geometry acceptance remains separately owned. No other proposal survives validation and no optional style preference is promoted into a finding.

Linux bundles/images and live degraded-terminal Node/MCP parity remain UNVERIFIED. Shared projection source supports parity but does not manufacture live evidence. Do not infer platform code defects or retrofit broader approval/verification requirements from these limits.

Skipped: implementation, duplicate builds/test lanes, fresh live/browser journeys and Linux/image execution. Risk: the reproduced authored identity loss remains; platform/live degraded evidence is still incomplete.
