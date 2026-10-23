# TASK-003-R7 — Complete range validation and authored SQL bind recognition

Kind: remediation. Status: proposed; route directly to `$wyrd-implement`.
Spec: [approved revision 13](../../spec.md). Original task: [TASK-003](../../tasks/TASK-003-shared-ui-foundation.md).
Remediates: **FIND-003-13**, **FIND-003-15**. Parent: TASK-003; follows R6.
Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`, branch `wyrd-ui-main-integration`.
Reviewed cumulative base: `c62ebce8c4de0d43a2b9d50d360de628c49fd533`; candidate: `21dab5d11e484ffceaa4718fc7370c7d6c67dc75`.

## Problem and intended result

Two shared input-planning boundaries still produce incorrect behavior. A relative range accepted by the package can resolve to bounds the HTTP hosts reject; a range-free table query containing literal `$1` text is refused before it reaches the query owner. Finish validation and recognition at their existing portable owners so every host agrees, while preserving exact authored SQL and the existing server security/query lifecycle.

For example, `{last:'100000000d'}` at the ordinary 2026 clock currently passes admission and resolves to a signed expanded-year string outside the existing RFC 3339 contract. It must instead produce the existing field-specific validation error before reads, unless resolution produces bounds that satisfy the existing absolute schema. Conversely, `SELECT '$1' AS label FROM vala.gateway.calls` contains literal data and no bind: an authorized direct Chart load without a range must reach the executor unchanged with empty parameters.

Independent diagnosis, caller traces, correction boundaries and focused proof: [findings-validation.md](findings-validation.md). Focused uncertainty resolution: [followup-review.md](followup-review.md). These are validated bounded behavior gaps under REQ-025/030 and original TASK-003, not new product decisions.

## FIND-003-13 — Successful range resolution violates its existing contract

Paths below prefixed `ui/` are rooted at `crates/wyrd/wyrd-server/wyrd-ui/`.

At `ui/packages/ui/src/lib/schemas.ts:67–75`, RelativeRangeSchema checks native Date capacity. At `:198–201`, resolveRange returns native ISO strings as AbsoluteRange without establishing AbsoluteRangeSchema (`:44–55`). Native Date accepts expanded signed years; the existing absolute schema does not. Source reproduction at `Date.parse('2026-10-22T00:00:00Z')` admits `100000000d`, returns `from:'-271764-02-09T00:00:00.000Z'`, and fails AbsoluteRangeSchema.

WyrdView resolves admitted root input before panel scheduling. Local/MCP loadPanel can execute a range-independent CardHeader; remoteLoader forwards the same generated context to both HTTP hosts, which refuse it before loading that CardHeader. Explicit Chart range admission can likewise generate malformed promised RFC 3339 binds even under a valid inherited range. R6 fixes the earlier Date exception but does not establish valid absolute output. The new test at `ui/src/lib/ui-library.test.ts:396` asserts admission alone, so green proof misses the mismatch.

Complete the shared range admission/resolution authority: every successful result must satisfy the existing AbsoluteRangeSchema at the shared resolution clock. Inability to produce valid bounds must use existing inputProblem/canonical validation and existing document/panel error states before executor work. A rejected root document displays its failed range field and performs zero reads; a rejected explicit Chart range fails only that panel and preserves valid siblings. Both HTTP endpoints must resolve through their existing honest refusal/loaded-error representation.

Reuse RangeSchema, AbsoluteRangeSchema, resolveRange and current validation/error display. The invalid state originates in conversion from authored relative input to executable bounds, so enforce the invariant there and connect its result to current callers. Do not widen the absolute contract, add a retention policy, repeat numeric guards at hosts, or insert an uncaught throwing parse into WyrdView.start. Preserve one inherited clock, explicit range precedence, ordinary 6h/24h and absolute ranges, direct example use, cancellation and exact SQL/binds. Private signatures/control flow remain implementation-owned.

## FIND-003-15 — Inert SQL text is treated as a parameter

At `ui/packages/ui/src/lib/loaders.ts:90–94`, chartParams matches raw SQL with `/\$([1-3])(?!\d)/g`. Its sole production caller loadChart (`:144–150`) returns an error before executor.run when that match finds a bind without a range. ChartInputSchema and LoadContext both support range-free table loading.

`SELECT '$1' AS label FROM vala.gateway.calls` is an otherwise supported table-bearing SELECT with no actual parameter. The literal currently causes a false missing-range refusal. Comment `$3` text can similarly manufacture bind positions and executed values. The original literal-only SELECT example is insufficient because Oracle independently requires a Bifrost table; retain that server floor. The supported-table example requires ordinary table-read authorization and demonstrates the gap without relaxing SQL validity or permissions.

Correct recognition once at the existing chartParams planning owner so only actual parameter tokens count. Conventional quoted literals/identifiers and comments, including relevant supported quoting/escape/delimiter forms, must not demand or manufacture range binds. Preserve authored SQL byte-for-byte, the true highest `$1..$3` parameter position/order, missing-range refusal for real placeholders, explicit precedence, and matching executed params/Open in Query handoff.

loadChart, public loadPanel, direct custom servers, renderer local/MCP loading and both HTTP hosts share this owner; no per-host correction is needed. Oracle parse_select_tables and DataFusion planning/with_param_values remain the read-only/table/binding authority. No existing portable tokenizer was found to reuse; the minimal conventional lexical recognition inside existing planning is sufficient. Do not create a SQL-validation framework, parser service, public bind field, dependency or transport, import native parsing into browser-safe entries, blindly bind all three positions, or drop real-placeholder validation when range is absent.

## Scope, preserved behavior and non-goals

Expected work stays in existing portable schema/range/loader owners, directly affected error consumers and focused tests. Use existing catalog generation if source changes require regenerated projections; never hand-edit generated artifacts. Preserve FIND-003-1..12 and -14, exact flat Card identity, positional query results, terminal safety, ceilings/stream release, session/credential/tenant authority, typed actions and separately accepted TASK-027 layout.

No new public contract, dependency, configuration, host transport, backend query/native/session behavior, arbitrary duration policy, Ask, saved views, A2UI or publishing. The two correction boundaries are independent but share the portable input-planning write set and can ship in one bounded remediation. A new material behavior/security/ownership decision returns to specification authority rather than being inferred here.

## Acceptance and focused proof

Extend current recording-executor/render/endpoint tests; no new harness or live-server prerequisite.

| Finding / scenario | Required observable proof |
|---|---|
| FIND-003-13 absolute output invariant | At fixed 2026-10-22 clock, `100000000d` cannot be successfully resolved outside AbsoluteRangeSchema. Every tested successful resolution satisfies the schema; rejected resolution reports the authored range field canonically. Keep the former Date-invalid counterexample. |
| FIND-003-13 root initial/replacement and portability | Initial bad root and valid-to-bad replacement show visible canonical range validation with zero new executor reads. Local/remote paths agree, including a range-independent CardHeader; no generated invalid context is forwarded as valid. |
| FIND-003-13 explicit panel and endpoints | Bad explicit Chart range under valid inherited bounds fails only that panel; valid sibling loads. Both existing BFF/custom POST harnesses resolve to their existing validation representation without IO or escaped rejection. |
| FIND-003-13 preserved resolution | Ordinary 6h/24h/absolute ranges, one inherited clock, explicit precedence, exact binds and cancellation remain correct. |
| FIND-003-15 inert SQL | Table-bearing quoted-literal, quoted-identifier and comment parameter text without a range reaches executor once with unchanged SQL and empty params; returned table state and executed values agree. Cover relevant conventional escape/comment forms. |
| FIND-003-15 real/inert mix | Actual `$1`/`$2` mixed with inert `$3` uses only the true highest parameter; actual placeholders without range still give existing validation. Ordinary three-bind planning, range precedence and Open in Query values remain exact. Exercise loadChart/loadPanel, not only a helper. |

Record meaningful RED/GREEN with actual selected counts. Use the existing narrow owners:

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Validated portable rendering$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Dependent loads and safe actions$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^Canonical UI problem projections$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec node packages/ui/scripts/catalog.ts --check
mise exec -- git diff --check
```

These pure TypeScript corrections need no native rebuild, broad aggregate, full journey suite or renewed release matrix in this task. Change review owns combined journey proof. Mandatory Rust documentation is already closed; do not create a documentation-only round. Subsequent task review reassesses the complete original cumulative task.
