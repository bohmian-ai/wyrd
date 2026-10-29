# Independent structured Ponytail validation

Disposition: **FIX_REQUIRED**. Validated active ledger: **FIND-003-13** and **FIND-003-15**. FIND-003-1..12 and FIND-003-14 remain closed. Both corrections fit approved revision 13; no new product or architecture decision is needed.

## Subject and method

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`, branch `wyrd-ui-main-integration`; cumulative base `c62ebce8c4de0d43a2b9d50d360de628c49fd533`, immutable candidate `21dab5d11e484ffceaa4718fc7370c7d6c67dc75`. Authority: approved spec revision 13, original `tasks/TASK-003-shared-ui-foundation.md`, root and TypeScript SDK AGENTS, spec-driven workflow, maintainer guide, relevant UI/error/Rust/testing guidance, Wyrd client/Card model, browser-session security boundary and Oracle query authority. No CodeGraph index exists.

Read both task-review reports, standards, maintainer, system, security-domain, focused follow-up and claim comparison. Independently inspected the cumulative changed owners, latest R6 locating diff, current complete correction-owner functions, public callers and sibling consumers, prior ledger/R6 and retained proof. Reviewed package/schema/catalog, loaders/view/actions, plotting/identity, adapters/hosts, native/shared collection/admission/lifecycle, session construction and packaging against their existing owners. Reports are claims, not source authority. Unrelated PR #205/main and TASK-027 layout remain excluded; foundation validation/loading seams remain in scope. Only this report was written.

## Claim decisions

| Proposed claim | Independent decision | Evidence and reconciliation |
|---|---|---|
| INV-RANGE-ABSOLUTE, SYS-1 and follow-up retaining FIND-003-13 | **CONFIRMED** | Relative admission and resolver success do not imply the existing absolute contract. Source probe below confirms a current-clock counterexample and callers prove host divergence. |
| Behavior/maintainer closure of FIND-003-13 | **REJECTED as complete closure** | R6 fixes the former Date-invalid exception. That narrower success is true, but does not discharge valid-absolute-bounds-or-canonical-validation, explicitly required by R6 and REQ-030. No second finding is created for another consumer. |
| BEH-1 and follow-up authored-SQL recognition claim | **REVISED**, retained as new FIND-003-15 | Raw regex counts inert text before execution. The original no-table SELECT is inadequate server reachability evidence; the corrected supported-table SELECT establishes the required path without changing the existing SQL floor. Correction is limited to current range-bind planning. |
| FIND-003-14 closure | **CONFIRMED CLOSED** | The candidate adds the required Panics section to the new client regression; its assertion body is unchanged. |
| Standards, maintainer and security empty proposal sets | No additional finding | Source supports their domain conclusions. Ordinary range validation/security correctness does not imply full behavioral range closure. |
| FIND-003-1..12 closure | **CONFIRMED CLOSED** | Producer/caller evidence below retains each prior correction. |

No unresolved source/authority conflict remains. The focused follow-up was justified by conflicting range closure claims and a unique SQL path. Its claims were validated independently rather than accepted by reviewer count.

UI-relative paths below are rooted at `crates/wyrd/wyrd-server/wyrd-ui/`; other paths are repository-relative.

## FIND-003-13 — Admitted relative input can resolve outside the absolute contract

**Status:** CONFIRMED. **Discovery IDs:** INV-RANGE-ABSOLUTE, SYS-1, focused follow-up's same cause. **Classification:** INCORRECT.

**Obligation:** REQ-030 shared validated portable loaders, RFC 3339 range binds and visible field-specific validation; original TASK-003 validated rendering/Chart schema-error acceptance; R6's valid absolute bounds or canonical range-field problem outcome. The absolute bound schema is already the contract; no additional range policy is being invented.

**Locations:** `packages/ui/src/lib/schemas.ts:44–55,67–75,198–201`; consumers `packages/ui/src/lib/WyrdView.svelte:240–256`, `packages/ui/src/lib/loaders.ts:144–150`, `packages/ui/src/lib/view.ts:54–64`, shipped `src/routes/t/[tenantKey]/ui/load/+server.ts:19–29`, custom `examples/ui-app/src/routes/view/load/+server.ts:8–12`. Current test `src/lib/ui-library.test.ts:396` expressly admits the counterexample but never checks its resolved absolute output.

**Reachability and evidence:** An artifact-free Node import of actual candidate `schemas.ts` produced:

```json
{"admitted":true,"resolved":{"from":"-271764-02-09T00:00:00.000Z","to":"2026-10-22T00:00:00.000Z"},"absolute":false}
```

Input was `{last:'100000000d'}`; the resolution clock was `Date.parse('2026-10-22T00:00:00Z')`. `RangeSchema.safeParse` succeeds. Native Date validity and `toISOString()` support expanded signed years; `AbsoluteRangeSchema.safeParse` refuses that returned result. This does not rely on an unusual injected clock, query timestamp implementation or unavailable data.

The full schema/span/resolver, `loadChart`, `loadPanel`, `validatePanel`, `validateView`, WyrdView load/start/settle paths, remote loader and both POST bodies were read. Shared root and explicit Chart admission accept the value. WyrdView resolves root bounds once before scheduling valid panels. Its local executor path then calls loadPanel; CardHeader performs exact Card/list reads and never consumes time bounds. MCP App supplies the same local executor path. A remoteLoader serializes those bounds unchanged, and both existing HTTP hosts refuse them through AbsoluteRangeSchema before dispatching any panel, including that same range-independent CardHeader. Therefore the very same accepted document can read under local/MCP and fail under HTTP. An explicit Chart range resolves through the same producer after validatePanel and can supply malformed bounds as promised RFC 3339 binds even when inherited context was valid. Direct custom-app loading is the remaining production resolver caller; its authored `24h` range must keep working. Chart brush admission already uses AbsoluteRangeSchema and is a consumer contract to preserve, not widen.

**Consequence:** R6 prevents the former escaping Date conversion for `1000000000000d`, but permits an invalid generated AbsoluteRange. Required hosts disagree and downstream errors identify generated context instead of the authored failed range. This is one incomplete source invariant, retaining ID13 rather than a new host-specific finding.

**Smallest safe correction:** Finish the existing shared range admission/resolution owner so a successful resolved result satisfies the existing AbsoluteRangeSchema at the actual shared resolution clock. Inability to produce it must take existing inputProblem/canonical validation and error-state paths before reads: root document error with zero panel reads; explicit Chart field error confined to that panel with valid siblings preserved; endpoints resolve to their existing honest refusal/loaded-error representation. Reuse the existing schemas, canonical problem and renderer/loader state mechanisms. A validation guard belongs at the producer converting untrusted relative text to executable bounds. Do not just add a throwing absolute `.parse()` to the currently uncaught root start path.

**Ladder and alternatives:** Deleting relative range support deletes required behavior. Existing range schemas/resolver plus native Date and canonical error states already cover the needed boundary; no dependency, configuration or service is needed. Do not widen RFC 3339 to expanded years, add an arbitrary retention cap, or duplicate arithmetic guards in hosts. Preserve one inherited clock, explicit precedence, ordinary relative/absolute values, exact SQL/binds, cancellation, valid siblings and TASK-027 layout. Private signatures and control flow stay implementation-owned. This is completion of approved validation, not a public contract expansion.

**Focused closure proof:** Extend existing pure range/render/endpoint cases with this Date-valid but contract-invalid span at the fixed clock above. Assert every successful resolution passes AbsoluteRangeSchema, or the canonical failed-field error occurs before IO. Cover initial/replaced root input, explicit bad Chart plus valid sibling, and both existing endpoint harnesses. Keep the former Date-invalid example, ordinary six-hour/24-hour/absolute cases, one inherited clock, explicit precedence and exact binds green. Existing selectors/harnesses suffice; no live-server rebuild or new framework is required.

## FIND-003-15 — Inert authored SQL text falsely demands range binds

**Status:** REVISED from BEH-1. **Discovery IDs:** BEH-1, focused follow-up authored-SQL claim. **Classification:** INCORRECT.

**Obligation:** REQ-025 table accepts any result shape; REQ-030 permits runtime authored read-only SQL and range-aware binds, preserves exact SQL/binds, and requires portable direct-component loading. TASK-003 owns Chart loading and authored SQL preservation. Range is optional in ChartInputSchema and LoadContext; a table read without real range parameters is a supported path. Existing server SQL validation remains authoritative.

**Locations:** `packages/ui/src/lib/loaders.ts:90–94` (`chartParams`), sole production caller `:144–150` (`loadChart`); schema `packages/ui/src/lib/schemas.ts:83–105`; public dispatch `loaders.ts:244–253`. Existing server authority: `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3164–3178,3374–3421`.

**Reachable example:**

```sql
SELECT '$1' AS label FROM vala.gateway.calls
```

This references an existing supported Bifrost table, with ordinary table-read permission and no actual bind. Its projected literal needs no payload permission or time range; either populated or empty output is a legitimate table result. The original `SELECT '$1' AS label` alone fails Oracle's existing at-least-one-Bifrost-table floor, so that example is explicitly rejected as server reachability proof. Correcting the example removes that defect in the discovery evidence without changing the diagnosed client producer.

**Source trace and evidence:** Actual candidate chartParams scans raw SQL with `/\$([1-3])(?!\d)/g`; quoted data, quoted identifiers and comments count exactly like placeholders. A source-schema probe confirms ChartInputSchema admits the table example. An artifact-free probe extracted and evaluated the exact regex from source: it reports highest position 1 for the literal example, 3 for `SELECT call_id FROM vala.gateway.calls -- $3 is explanatory text`, and 3 for an actual `$1` plus comment `$3`. With no range, chartParams immediately returns the canonical validation problem. loadChart then returns error props before executor.run; loadPanel validates and delegates to that same loadChart. Thus this is a reachable false local refusal of an otherwise valid read, not merely a stylistic regex concern. The primary consequence needs no assumption about how DataFusion treats unused bound values. With a supplied range the same planner also manufactures bind positions from inert text, and exposes those values as executed params.

The existing public direct loader/custom server host may supply an executor without a range for a range-independent table. The example application uses the same direct loadChart mechanism (its own fixed range is not itself the counterexample). WyrdView's local path, both HTTP hosts and AppBridge/MCP share loadPanel/loadChart; Chart's Open in Query consumes the same `executed` SQL/params. chartParams has one production caller; no other package/app/example parameter recognizer exists. All full correction-owner and dispatch bodies were read. The cumulative task adds this file/algorithm; this is neither inherited PR #205 content nor TASK-027 layout, nor the earlier explicit-range precedence finding.

Oracle's parse_select_tables still owns single read-only SELECT/table-reference validation; plan_logical uses DataFusion SQL planning then with_param_values on the parsed plan. Literal/comment contents are not placeholder expressions there. Node and MCP adapters carry authored SQL and planned params unchanged. Neither host guards nor changing server binding can repair a query prevented from reaching the executor.

**Smallest safe correction:** Correct recognition once inside the existing chartParams range-bind planner to identify real parameter tokens and ignore quoted literals/identifiers and comments under the SQL forms accepted by the existing floor. Use conventional lexical treatment for this narrow recognition, including relevant quoting/escape/comment delimiter forms, while leaving SQL byte-for-byte unchanged. Preserve true `$1..$3` positional ordering, missing-range refusal for actual placeholders, range precedence, executed-value/action handoff and all existing query/admission/lifecycle behavior. Do not treat the absence of range as permission to ignore real placeholders; do not simply bind all three for every query.

**Ladder and alternatives:** Deleting bind planning removes required range-aware behavior. Existing chartParams is the owner to extend; there is no reusable portable tokenizer in the package/app sources or its Zod runtime dependency. Oracle's existing parser is the SQL authority, but pulling Rust/native planning into this browser-safe portable entry point violates the boundary. A small conventional recognition correction here is sufficient. A new SQL validator/parser service, public bind field, dependency, transport or broad SQL framework is unnecessary and outside this recommendation. The correction must not reimplement or loosen read-only/table/permission checks. No architecture or public contract decision remains for implementation.

**Focused closure proof:** In existing pure loader cases, pass the supported-table-shaped literal/comment queries without a range: executor called once with identical SQL and `params: []`, truthful table state and matching executed values. Mix actual `$1`/`$2` with inert `$3` and assert only the true highest position determines binds; cover conventional escaping/comment controls supported by the query owner. Retain actual-placeholder/no-range validation, ordinary three-bind/range precedence and Open in Query exact-value assertions. Exercise loadChart/loadPanel, not only the helper. Existing recording executor and focused test selector suffice; no SQL server/new harness/native build is required for this planner fix.

## Prior closure retained against current source

| Prior finding | Independently inspected producer and consumers | Disposition |
|---|---|---|
| FIND-003-1 | loadChart chooses input.range before context.range; WyrdView resolves shared root once; request and Chart action use the same executed params | CLOSED; ID13 is the separate postcondition |
| FIND-003-2 | shapeChart, timeSeries, barRows and explicit stat lookup use shared first-position columnIndex; table retains positional cells | CLOSED |
| FIND-003-3 | Chart bounds include zero with signed increasing domain; bars span signed value to zero with nonnegative drawn height | CLOSED |
| FIND-003-4 | MAX_SERIES=20, fold only above twenty into nineteen plus aggregate; four marker/five dash combinations shared by plot and legend | CLOSED |
| FIND-003-5 | CardHeader consumes named version/tab states, blocks pending/unavailable controls, resets select to host version and renders ActionNote; WyrdView validates/deduplicates host intents | CLOSED |
| FIND-003-6 | Node closes post-acquisition aborted stream, listens during collect and checks after collect; native close notifies before mutex wait; WyrdView abort/fences results; Card listing checks before each dependent page | CLOSED |
| FIND-003-7 | Rust-derived problem JSON feeds wyrdProblem; failed preserves denied state; both refusal helpers serialize complete canonical problems; ID13 is invalid-state production, not a reopened problem catalog issue | CLOSED |
| FIND-003-8 | contract.ts JSON import uses standard import attribute; browser SDK imports are type-only; public smoke remains in the owning test task | CLOSED |
| FIND-003-9 | timeSeries chooses aggregate caption against final retained authored captions; plot, legend and tooltip consume the same output | CLOSED |
| FIND-003-10 | typed JSON [group,y] keys precede summation; collision qualification keeps null/text-null and distinct multi-y groups separate | CLOSED |
| FIND-003-11 | native original Option<f64> -> lossless decimal integer parse -> shared admit_ceilings after taking stream; refusal drops stream; server uses same policy; shared JsonResultBuilder preserves exact JSON accounting/positional values while existing owners retain protocol/terminal/settlement | CLOSED |
| FIND-003-12 | shared fmtCardRef includes kind/name/version and supplied UID; Header/Chip reuse formatter; version intents omit prior UID | CLOSED |
| FIND-003-13 | R6 rejects former Date-invalid duration, but successful output can still fail the existing AbsoluteRangeSchema at a current clock | ACTIVE, CONFIRMED |
| FIND-003-14 | client.rs:760–765 contains required Panics conditions; R6 latest diff preserves all three expect assertions | CLOSED |

## Remediation and proof boundary

One bounded R7 task can package ID13 and ID15 in existing portable range/schema/loader owners and their immediate error consumers/tests. Keep all unrelated native/query/session/security/packaging owners unchanged. Regenerate catalog projections only through the existing source generator if needed; no hand-edited generated outputs. Preserve all corrected prior behaviors and explicit non-goals. The existing real-server integration evidence remains credible for its asserted paths but does not cover these counterexamples.

Recommended narrow existing commands after extending assertions (not rerun by this validator):

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Validated portable rendering$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Dependent loads and safe actions$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^Canonical UI problem projections$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec node packages/ui/scripts/catalog.ts --check
mise exec -- git diff --check
```

Record actual RED/GREEN and selected counts. No broad aggregate/full journey requirement belongs in this pure remediation; change review owns combined journeys. Mandatory Rust documentation is now closed and does not need a separate round.

## Evidence limits

Supplied green UI check/test 40/40/catalog, fmt/lints, focused client nextest and real-server integration 2/2 are reported executions. This validator ran only read-only Git/source inspection and artifact-free Node source/schema and regex probes. Those probes prove the shown producer conditions; caller source ordering proves their propagation. No fresh mounted browser, endpoint execution, full suite, live server, image/native platform matrix or human visual acceptance is claimed. Historical evidence is not renewed qualification. HEAD was rechecked against the immutable candidate before completing this report.

Skipped: build/test/deployment reruns under packet-only restrictions. Risk: admitted relative bounds still diverge across hosts, and valid range-free SQL with inert parameter text is falsely refused.
