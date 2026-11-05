# Focused uncertainty review

Result: **RESOLVED** for both assigned uncertainties. Propose retaining FIND-003-13 with its absolute-contract consequence and retaining BEH-1 with the reachable SQL example corrected below. These are discovery conclusions for independent validation, not the final verdict.

Subject: base `c62ebce8c4de0d43a2b9d50d360de628c49fd533` through candidate `21dab5d11e484ffceaa4718fc7370c7d6c67dc75`; approved spec revision 13; original TASK-003 and R6. HEAD still matched candidate at completion. Read subject, discovery claims, actual cumulative changes of the disputed owners and their callers, REQ-025/030, original task/R6, repository rules, maintainer guide and workflow. Scope is these uncertainties only; this focused pass does not replace cumulative discovery coverage.

Paths below prefixed `ui/` are rooted at `crates/wyrd/wyrd-server/wyrd-ui/`.

## Range closure conflict

**Finding source:** INV-RANGE-ABSOLUTE and SYS-1; retain **FIND-003-13**, classification INCORRECT. The behavior and maintainer reports correctly establish closure of the original invalid-Date exception example, but that narrower fact does not establish R6's required postcondition. R6 explicitly says admitted input must resolve to valid absolute bounds or the existing field-specific validation problem, and selects the existing absolute-range contract rather than native Date validity alone.

**Source and reachable state.** `ui/packages/ui/src/lib/schemas.ts:70–75` accepts a span whenever `new Date(-span(last)).getTime()` is not NaN. `resolveRange` at lines 198–201 returns `toISOString()` results under the declared `AbsoluteRange` type without checking `AbsoluteRangeSchema` (lines 44–55). Native Date can encode signed expanded years that the existing RFC 3339 schema rejects. An artifact-free Node probe imported the actual `schemas.ts` source, not dist:

```text
RangeSchema.safeParse({last:'100000000d'}).success = true
resolveRange(parsed.data, Date.parse('2026-10-22T00:00:00Z'))
= {from:'-271764-02-09T00:00:00.000Z',to:'2026-10-22T00:00:00.000Z'}
AbsoluteRangeSchema.safeParse(resolved).success = false
```

This uses an ordinary current clock, not an exotic supplied clock. The newly added test expressly admits `100000000d`, explaining why the supplied green checks miss the gap.

**Producer and consumers.** WyrdView's shared root parse accepts this range; `start` at `ui/packages/ui/src/lib/WyrdView.svelte:240–256` resolves it once before scheduling panels. Its derived loader dispatches to either `loadPanel` plus the host executor or a supplied remote loader. `view.ts:54–64` serializes the same malformed resolved bounds without changing them. Both shipped `ui/src/routes/t/[tenantKey]/ui/load/+server.ts:19–29` and custom `examples/ui-app/src/routes/view/load/+server.ts:8–12` reject those bounds with `AbsoluteRangeSchema` before panel dispatch. A CardHeader, whose loader never consumes a time range, can execute through the local/MCP host while the same valid document's CardHeader is refused by HTTP. A range-independent Chart is another sibling; no query-engine timestamp interpretation is needed to prove divergence. Explicit Chart ranges also reach `loadChart:145`, which resolves with the same unchecked producer before planning binds. The direct custom-app page imports the same resolver for its ordinary 24h range. `chart.range` action admission uses `AbsoluteRangeSchema` too; do not weaken it to accommodate malformed output.

**Attribution and correction boundary.** The schema/resolver are added by the cumulative TASK-003 foundation, with R6 changing admission and moving the resolver to the schema owner. This is a continuation of FIND-003-13's shared admitted-input/postcondition cause, neither PR #205 nor TASK-027 layout. Existing mechanisms suffice: `RangeSchema`, `AbsoluteRangeSchema`, `resolveRange`, canonical `inputProblem`, existing document validation display and panel error props. At the shared producer, require resolved bounds to satisfy the existing absolute contract at the resolution clock and surface the canonical field error before reads. Apply that outcome through root document and explicit Chart paths: root fails visibly with zero panel reads; an invalid explicit range fails only that panel. Preserve the single inherited clock and valid siblings. Do not widen RFC 3339, impose an arbitrary retention cap, add per-host guards, or merely throw `AbsoluteRangeSchema.parse` into the existing uncaught root path. Private signatures/control flow remain implementation-owned.

Focused closure proof should establish the schema/resolver postcondition with this exact span/current fixed clock; initial/replaced invalid root, invalid explicit panel plus valid sibling, both existing endpoint harnesses, ordinary ranges and precedence. No new live-server harness is required.

## Authored-SQL token recognition

**Finding source:** BEH-1, classification INCORRECT; independent validation assigns the next unused stable ID if retained. The reported raw-regex defect exists in actual source. However, **`SELECT '$1' AS label` alone is not sufficient proof of an otherwise valid server query**: Oracle's existing floor requires at least one Bifrost table (`crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3374–3421`). Revise the example to an authorized table read such as:

```sql
SELECT '$1' AS label FROM vala.gateway.calls
```

or:

```sql
SELECT call_id FROM vala.gateway.calls -- $3 is explanatory text
```

These SELECTs reference a real supported Bifrost table, introduce no actual placeholder, and require only the ordinary relevant table-read authority. They can return populated or empty results without requiring a time range. The defect does not rely on accepting a query the Rust floor prohibits.

**Source and path.** `ui/packages/ui/src/lib/loaders.ts:90–94` searches raw SQL using `/\$([1-3])(?!\d)/g`, including quoted strings, identifiers and comments. No range produces a canonical validation refusal before `executor.run`; a range produces a bind list whose highest position can come from inert text. `ChartInputSchema` permits a table chart with SQL and no range; `LoadContext.range` is optional. `loadPanel` validates then dispatches `loadChart`; `loadChart:145–150` calls `chartParams` (its only production caller) before the executor. Public direct loading is explicitly required by TASK-003 and the custom-app design. WyrdView, shipped/custom endpoints and MCP App also share `loadPanel`/`loadChart`; Chart's executed-value output feeds Open in Query. Thus correction belongs once at the shared planner, not individual hosts or action consumers.

The artifact-free source probe read the actual `loaders.ts` regex and evaluated that expression in memory. It counted one bind for literal `$1`, three for comment `$3`, and three for actual `$1` plus literal `$3`. Applying the same source regex to the revised table queries gives the same false counts; the surrounding missing-range branch visibly refuses them before execution. No SQL server or dist-only behavior is assumed. The full chartParams and loadChart/loadPanel bodies were inspected.

**Existing authority and minimal correction.** Oracle `plan_logical` at lines 3164–3178 invokes existing DataFusion SQL planning and `with_param_values`; its parser at 3374 onward owns read-only/table validation. SQL literals are data there, not parameter nodes. Preserve those owners and exact authored SQL. No existing portable SQL parameter tokenizer was found in the package, app loader code or installed package dependencies (the UI package depends on Zod, with Svelte/SDK/MCP peers). Importing Rust/native parsing into the browser entry point is not a valid reuse. Extend the existing narrowly scoped chartParams recognition to count actual parameter tokens while ignoring conventional quoted text/identifiers and comments, including their supported escape/delimiter forms. This is lexical recognition solely for existing range-bind planning, not a new SQL validator/parser service, transport, dependency or public input contract. Do not change read-only admission or bind interpretation in Rust. Do not simply return empty params whenever range is absent: retain the existing truthful missing-range refusal for real `$1..$3` placeholders and correct inert-text bind manufacturing when a range exists.

Focused proof: an authorized-table-shaped SQL string with inert `$1`/`$3` loads once with empty params and unchanged SQL without a range; mixed actual/inert tokens bind only the true highest position. Exercise loadChart/loadPanel output and executed values, ordinary real placeholders, quote escaping/comments and missing-range errors in the existing focused pure loader tests. Pure planner proof needs no new live-server/native build; existing server parser and binding remain unchanged.

## Limits

Only this report was written. No builds, test suites, live server, browser or cross-platform checks were run; supplied green results remain reported evidence. Fresh independent validation must decide the final ledger. The source-backed probes corroborate both gaps; they are not renewed deployed journey proof.
