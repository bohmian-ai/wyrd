---
id: TASK-003-R1
kind: remediation
status: ready
spec: SPEC-wyrd-ui-main-integration
spec_revision: 12
parent_task: TASK-003
remediates: [FIND-003-1, FIND-003-2, FIND-003-3, FIND-003-4, FIND-003-5, FIND-003-6, FIND-003-7]
---

# Correct foundation chart, action, cancellation and error contracts

Execute with `$wyrd-implement` under the approved specification. This is a bounded remediation, not authorization to revise product behavior.

Specification: `changes/active/wyrd-ui-main-integration/spec.md`, revision 12 approved.
Original task: `changes/active/wyrd-ui-main-integration/tasks/TASK-003-shared-ui-foundation.md`.
Review base: `695ca914df61a8683059f6f32279ec2148424e06`.
Reviewed candidate: `ab1d2499f6a08a6b5afe1f6c904e72746d526bda`.
Verdict and validated ledger: `verdict.md` and `findings-validation.md` beside this task.
Requirements: REQ-025/030, original TASK-003 action/loading and complete chart obligations, canonical repository error rules. Preserve revision-12 TASK-027 behavior; its independent remediation is separate.

UI paths below are rooted at `crates/wyrd/wyrd-server/wyrd-ui`.

## Diagnosis and selected correction boundaries

### FIND-003-1: explicit panel bounds lose to inherited bounds

`packages/ui/src/lib/loaders.ts:160` chooses context.range before input.range. WyrdView and both remote endpoints always supply inherited bounds, so an explicitly bounded panel queries the document interval instead. Open in Query then reproduces those wrong binds. Direct calls without context behave differently. This violates REQ-030's explicit panel override; existing tests prove only common-range behavior.

Correct precedence once in shared loadChart: an explicit panel range wins, using the existing relative-range resolver when needed; absent override uses the already resolved inherited bounds. Brush/reset still reload every panel and change inherited bounds; they do not erase explicit author input. Preserve exact SQL and typed parameter handling. Do not add host-specific policies or backend changes.

### FIND-003-2: duplicate columns have conflicting binding authorities

`loaders.ts:127-131` validates the last matching descriptor via Map; `plot.ts::columnIndex` and stat/timeSeries/barRows consume the first. `[v:Utf8,v:Int64]` with `['bad',3]` is accepted populated but reads text as a number, while reversed types reject a usable binding. Both executors legitimately preserve duplicate names positionally; tables are correct. Existing duplicate coverage only proves positional table/transport preservation.

Reuse the existing first-match columnIndex selection for validation at the shaping producer. Preserve result arrays, duplicate names and table rendering. Do not rename/deduplicate columns, reject every duplicate or add independent renderer guards. Cover named x/y/series roles where applicable through the same selection rule.

### FIND-003-3: signed numeric chart geometry is invalid

`components/Chart.svelte:44-70,118-149` computes niceCeil of negative maxima, producing NaN for negative timeseries; bars use signed values directly as rect heights, hiding valid negative data. Numeric schemas admit signed Int/Float/Decimal and the spec imposes no nonnegative restriction. Positive fixture success cannot prove this path.

Correct signed domains at Chart's existing geometry owner. Domains must remain finite/increasing for mixed, all-negative, flat-negative and zero data; signed bars include a zero baseline and nonnegative rectangle sizes on the correct side of zero. Preserve positive-only appearance, null handling, dense labels, resizing, source/footer and actions. Do not reject signed inputs, hide them with SVG guards or add a chart/scale framework.

### FIND-003-4: series fold at five instead of twenty-one

`plot.ts:14-15,47-54` limits output to four because Chart has four styles. Five through twenty distinct series lose their identities contrary to REQ-025; the current five-series test encodes the wrong threshold.

Use the approved twenty-series boundary in existing timeSeries and make existing Chart style selection valid for every output series, retaining non-color distinguishing cues, labels/tooltips, deterministic grouping/order and aggregate values. Raising only the constant leaves unsafe style indexing and is incomplete. Do not introduce another cap or plotting dependency. Correct the conflicting test to the specification.

### FIND-003-5: version/tab controls ignore action outcomes

`components/CardHeader.svelte:55-60,103-113` emits card.version/card.tab, but reads no corresponding ActionState. WyrdView validates/stores results and direct hosts can supply them; only Observe consumes its entry. Refusal/error/unavailable reasons disappear, direct consumers can emit duplicates during pending, and a refused select can show a version inconsistent with authoritative Card props. Chart-only action tests miss this.

Consume the existing named states at the controls, following ActionButton's established feedback convention while preserving select/tab semantics. Pending/unavailable prevents interaction; refusal/stable errors, reasons and supplied completion feedback remain visible. Identity/selection must remain truthful to authoritative props following refusal. No new dispatcher, production route, mutation service or host workaround.

### FIND-003-6: startup abort misses the acquired stream

`packages/ui/src/lib/node.ts:29-36,49-57` checks signal before awaiting connect/stream, then attaches a listener after startup. Abort during those awaits is not replayed, collect proceeds, and run can return success. SDK BifrostQueryStream.return/native close already owns supported cancellation. Both BFF and custom remote endpoint forward signals; renderer stale-result guards protect display but not resource release. Historical live test uses an already-aborted signal and cannot prove this race.

At the Node adapter's stream-acquisition boundary, observe cancellation when the stream becomes available, close it through existing return, and refuse collection/success. Retain active-collection cancellation/listener cleanup and ensure cancellation winning before completion cannot return usable rows. Keep other reads and MCP signal forwarding unchanged; no retry, alternate transport or renderer cancellation layer.

### FIND-003-7: local UI errors and HTTP projection bypass canonical authority

`loaders.ts:105,119,234-242,280`, `mcp.ts:24-29`, `view.ts:63`, `WyrdView.svelte:69,201,237` produce uncatalogued WYRD_UI_* identities through public loaders/components. First-party `src/routes/t/[tenantKey]/ui/load/+server.ts:33-34` and repository `examples/ui-app/src/routes/view/load/+server.ts:15-16` serialize reduced four-field problems, omitting canonical type/details/remediation. Green tests explicitly expect some invented codes or only subsets.

Reuse existing derive-backed errors in `crates/wyrd-spec/src/error.rs`, its generated browser-safe `schemas/ui_problem_examples.json`, and existing BFF `$lib/server/problem.ts::problem/safeProblem` conventions. Expose necessary generated metadata through current package/build ownership, without importing a first-party server module/native SDK from browser exports or copying a handwritten catalog. Local validation/internal failures use appropriate canonical projections; diagnostic field/component detail remains safe. HTTP errors preserve full canonical metadata. Preserve exact upstream permission/error identities, denied-state mapping and terminal behavior; do not expose internal exception strings in new HTTP diagnostics. Reduced display props need not become full HTTP problems. No new backend error domain or framework.

## Preserved behavior and scope

Own only the shared UI shaping/plot/component/executor/error producers, directly affected endpoints, tests and generated-projection/package wiring needed above. Keep shared JsonResultBuilder, SDK transport and server auth/audit/tenancy owners intact. No new dependencies, Card kinds, durable contracts, query service, persistence, saved views, Ask/A2UI, production MCP registration, npm publication or unrelated cleanup. Input documents stay immutable. Keep browser/native separation, original four-host consumption and TASK-027 layout/no-reload behavior. Coordinate any shared WyrdView edits with TASK-027 remediation without reverting its changes.

No new product, cross-service, resource-ownership or persistent-data decision is authorized. Stop for such a conflict rather than expanding remediation. Linux/images and live degraded-terminal proof remain outstanding evidence at their owning verification stage; do not invent a platform code defect or demand broad release/journey aggregates here.

## Acceptance and focused proof

Use scenario-by-scenario Red/Green with the repository toolchain. Extend existing focused UI scenarios where suitable; add these specifically named regression cases for otherwise uncovered adapter/endpoint boundaries. Test layout/fixture mechanics remain implementation-owned.

| Finding | Required observable proof |
|---|---|
| FIND-003-1 | Explicit/inherited two-panel exact binds before/after brush/reset; relative override with fixed clock |
| FIND-003-2 | Both conflicting duplicate-type orders; valid first binding renders, invalid first binding errors; stat and representative plot x/y/series bindings; positional table unchanged |
| FIND-003-3 | Mounted negative/mixed/flat-negative/zero lines and bars have finite coordinates, nonnegative rect sizes and correct zero-side placement; positive/dense regressions stay green |
| FIND-003-4 | 5/20 series remain individual, 21 folds under approved rule; mounted legend/path/tooltip access works and aggregate values remain correct |
| FIND-003-5 | Direct controls suppress pending duplicates; denied/error/unavailable/success feedback delivered; one result through WyrdView; exact references/tabs and authoritative selection preserved |
| FIND-003-6 | Deferred startup aborted before stream return closes once, never collects/succeeds; during-collection and pre-aborted cases retained |
| FIND-003-7 | Local fallback errors use catalog-backed code/title; HTTP malformed-panel errors carry canonical metadata and safe context; upstream denial identity/state and browser export closure preserved |

Exact focused commands (new cases must exist and select real tests):

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Validated portable rendering$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Dependent loads and safe actions$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^Node query acquisition cancellation$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^Canonical UI problem projections$'
```

Complete only the narrow affected final lanes:

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui --filter @wyrd/example-ui-app check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui --filter @wyrd/example-ui-app build
mise run codegen:check
mise exec -- git diff --check
```

Perform a bounded signed-geometry browser spot-check with existing browser tooling. No new harness/dependency or full journey/release aggregate. Record exact RED failure, GREEN outcome, final commands and limits. Later review must reassess the complete cumulative original task and preserve FIND IDs.
