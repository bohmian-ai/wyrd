# Independent findings validation

Result: **FIX_REQUIRED**. The active deduplicated ledger contains **FIND-003-13** and **FIND-003-14**. FIND-003-1 through FIND-003-12 are closed at this candidate, including R5's lossless native budget admission. One bounded remediation should correct range validation and include the small rustdoc correction; no specification revision is needed.

## Subject and independent method

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`, branch `wyrd-ui-main-integration`; cumulative base `c62ebce8c4de0d43a2b9d50d360de628c49fd533`, candidate `6aace6e07728e59deb1fcd85ce969b2d0a4260db`. HEAD independently matched this full candidate. Authority is the approved revision-13 `changes/active/wyrd-ui-main-integration/spec.md`, original `tasks/TASK-003-shared-ui-foundation.md`, repository rules, and applicable architecture. Prior findings and remediation were hypotheses, not proof.

Read both implementation reports, standards, maintainer, system, security-domain and focused follow-up reports, claim comparison and verification limits. Independently inspected cumulative changed owners and actual current function bodies, producer/caller/sibling paths, original-foundation attribution, native and shared admission, projection, renderer/action boundaries, and existing proof. No CodeGraph index exists. The standards note is explicitly validated below rather than silently omitted. PR #205/main and accepted TASK-027 layout implementation are excluded as findings; the shared loading/validation seam remains TASK-003 scope.

Applicable authorities inspected include root and TS SDK AGENTS, spec-driven workflow, maintainer and Rust guidance, UI guidance, REQ-025/030 and the View validation contract, Card identity and browser-session authority. No proposed correction changes tenant/session authority, query execution, persistence, resource ownership or approved public behavior.

## Claim decisions

| Discovery source | Decision | Source-based disposition |
|---|---|---|
| INV-RANGE; corroborated/narrowed by focused follow-up | **CONFIRMED**, FIND-003-13 | The shared relative schema accepts an unresolvable value. Root resolution throws outside panel promise handlers; explicit Chart resolution rejects into an internal error or an unhandled endpoint promise. One producer owns these consequences. |
| NOTE-STD-1 | **REVISED**, FIND-003-14 | The descriptive test rustdoc exists, but the specifically required `# Panics` documentation does not. The repository's explicit rustdoc gate is stronger than the note's non-blocking classification. Batch this correction with the behavioral remediation. |
| Behavior/maintainer/system/security empty material proposal sets | No additional retained finding | Their ordinary-path acceptance and R5 conclusions are source-supported; broad validation PASS claims are narrowed by the independently demonstrated range counterexample. |
| FIND-003-1..12 closure claims | CLOSED | Current-source closure is recorded below; no prior finding is renumbered or reopened. |

There is no unresolved report conflict. The focused follow-up established the otherwise unexamined reachable range path. The disagreement concerns scope of existing ordinary-path proof, not contradictory approved authority. Its fixed direct-example range is `24h` in current `examples/ui-app/src/panels.ts`.

## FIND-003-13 — Unresolvable relative range bypasses visible validation

**Status:** CONFIRMED. **Discovery IDs:** INV-RANGE, focused follow-up's same cause. **Classification:** INCORRECT. **Violated obligation:** REQ-030's shared validated loaders and document contract, original TASK-003 scenario 1 and acceptance requirement for visible Chart schema errors. Specification lines 742–748 require an invalid panel to name its component/failed field while valid sibling panels still render. Invalid shared document input must produce the existing visible document error, rather than throw before loading begins.

**Exact locations:** UI-relative `packages/ui/src/lib/schemas.ts:56–61`, `packages/ui/src/lib/loaders.ts:75–78,156–162`; consumers `packages/ui/src/lib/WyrdView.svelte:81–89,240–256`, `src/routes/t/[tenantKey]/ui/load/+server.ts:18–29`, and repository-root `examples/ui-app/src/routes/view/load/+server.ts:8–12`.

**Evidence and reachable state.** `{last: "1000000000000d"}` matches the only relative-value constraint, `^[1-9]\d*[mhd]$`. Both root `ViewDocumentSchema` and explicit Chart input use that same RangeSchema. `resolveRange` multiplies its numeric prefix by 86,400,000 and calls `new Date(now - amount).toISOString()` without checking whether the instant exists. At the ordinary candidate clock this is an invalid Date, and ISO conversion throws `RangeError`. The issue requires only untrusted view/panel JSON; no server query, synthetic load, production MCP registration or alternate credential is necessary.

The complete correction-owner bodies and all direct resolver callers were inspected:

| Producer/caller/consumer | Actual propagation |
|---|---|
| RelativeRangeSchema -> RangeSchema -> root ViewDocumentSchema | `safeParse` succeeds, so the renderer's `!parsed.success` error branch cannot display this invalid range. `validateView` also returns it as valid. |
| WyrdView effect -> `start` | `start` first aborts previous controllers, then resolves the shared range before mapping panels or installing each promise's rejection handler. The exception escapes this effect; no panel read is scheduled, even for a CardHeader. This invalid root document need not execute otherwise-valid panels, but it must fail visibly and canonically. |
| Chart input schema -> `loadChart` -> `loadPanel` | Explicit input range is correctly selected ahead of inherited context, but resolution happens before planning/executor work. The async loader rejects. WyrdView's per-panel handler maps it to an internal failure rather than a field-specific validation problem; other panels can continue. |
| BFF and custom-app POST -> `loadPanel` | Both validate inherited absolute context only, then directly await the shared loader without a range-error projection. A valid context does not validate an explicit Chart range. The same error bypasses their normal refusal/loaded-props responses and rejects the request handler. This is a request/framework failure, not a Rust process crash or tenant escape. |
| Direct custom-app server page -> `resolveRange` | Its authored constant is `24h`, so it is not a separate attacker-controlled counterexample. It remains a required consumer to preserve if internal range resolution changes. |
| `mountView`, AppBridge result callback, `remoteLoader` | Mount/AppBridge pass the document into WyrdView without another range admission owner. Remote loading serializes the resolved range and interprets existing responses; adding independent arithmetic there would duplicate the source correction. |

Search covered direct `resolveRange`, `loadChart`, `loadPanel`, shared range schemas, and endpoint/example consumers. No other relative range validation owner recovers the invalid value. The existing absolute schema already uses native Date parsing and ordering; `inputProblem`, canonical `wyrdProblem('validation', ...)`, Loaded error props and renderer EmptyState already supply the required error mechanisms.

**Attribution.** The cumulative diff introduces this schema/resolver. Independently inspected `git show b0a05de50:.../loaders.ts` retains the original foundation's unchecked arithmetic/ISO conversion. This cause is not TASK-027 layout work or unrelated PR #205/main. It is separate from closed FIND-003-1, whose explicit-range precedence is correct.

**Observable consequence.** Schema-admitted root input throws before the document's panels load and can abort an existing generation without a visible document validation result. Panel-local input loses the required validation identity/field, and the two HTTP hosts can fail through the framework rather than their declared panel error surface.

**Decision-complete correction.** Extend the existing shared range admission/resolution owner, using native Date validity and the existing absolute-range contract. A relative input must either resolve to valid absolute bounds or produce the existing canonical validation problem naming the range field before invalid ISO conversion escapes. Reuse that single result in the existing document and Chart validation/loading paths: invalid root range displays a document validation error and schedules no reads; invalid explicit Chart range remains a field-specific panel validation error while valid siblings load; both endpoints return their existing honest problem/loaded-error representation without rejected handlers. A validation refusal at this boundary belongs here because the foundation owns conversion from untrusted range to executable absolute bounds.

This selects extension of current schema/resolver and existing problem/error-state mechanisms. Do not add a duration policy, retention cap, new error family, parser/service, configuration, repeated per-host numerical checks, blanket internal-error catch as validation, or replacement orchestration. Private signatures/control flow remain implementation-owned. Preserve one shared resolution clock per inherited view load, valid ordinary/absolute ranges, input-first precedence, unchanged SQL/typed binds, request cancellation, sibling isolation, credential boundaries and TASK-027 layout behavior. Generated catalog changes, if any, go through its existing generator.

**Ponytail ladder.** Removing relative ranges removes specified behavior. Existing shared schema/resolver is the correct owner; native Date plus existing absolute schema and canonical validation/error states cover the outcome. No new dependency or architecture decision is needed. Repeated downstream guards can be deleted from the recommendation because source admission/resolution owns the invariant.

**Focused closure proof.** Extend existing pure rendering/load/error tests, using the same counterexample and ordinary fixed-clock controls. Assert invalid root range displays the canonical validation identity and field with zero reads and no escaped effect error, including replacement of a previously valid document. Assert one explicit invalid Chart range reports validation and preserves a valid sibling. Exercise both imported endpoint handlers in the existing canonical-problem harness with a valid inherited range and invalid explicit Chart range: each resolves to its documented error representation with the validation identity/field and no executor read. The custom-app handler may construct its normal client; use the existing test/runtime setup rather than adding a credential mechanism. Retain valid six-hour/fixed-clock, absolute and explicit-precedence behavior. A tiny source-input proof is sufficient; no new server/journey harness or broad suite is required.

## FIND-003-14 — New browser-session regression omits required panic documentation

**Status:** REVISED from NOTE-STD-1. **Classification:** VIOLATION. **Exact location:** `crates/shared/wyrd-client/src/client.rs:760–774`, `browser_session_bifrost_connects_without_an_ingest_channel`.

**Violated rule and evidence.** Root AGENTS explicitly requires `# Panics` when a changed Rust item can panic. `agent_docs/rust.md:235–240` expressly includes tests and requires panic documentation. The new function has descriptive rustdoc, but no `# Panics`; its complete body uses `expect` for session construction, client construction and query-only connection. The nearby pre-existing browser-session regression documents such test panic conditions. The cumulative diff adds this function, so attribution is direct.

**Consequence and gate reconciliation.** No runtime/security regression is claimed. This is missing mandatory rustdoc content on a changed item, under the skill's explicit exception that keeps missing rustdoc blocking. Existing prose about intent does not document the assertion panic conditions or waive that rule. The prohibition on a style-only round does not require ignoring it: one behavioral remediation is already required for FIND-003-13, and this documentation-only correction belongs in that same task. It introduces no second implementation round.

**Smallest correction and proof.** Complete this test's existing rustdoc with the required panic conditions, preserving its assertions and runtime behavior. Reuse the nearby documentation convention. No caller, helper, test logic, runtime API, dependency or new test is needed. Static inspection of the changed block and format checking supply the proof; do not add a runtime test to prove documentation or rerun unrelated journeys for this correction.

## Current-source prior finding closure

| ID | Independently inspected producer and sibling-consumer evidence | Disposition |
|---|---|---|
| FIND-003-1 | `loadChart` selects explicit input range before inherited context; Chart request and Open in Query consume the same `executed` binds. WyrdView resolves inherited range once; both endpoints reuse loaders. Representability is the separate new cause above. | CLOSED |
| FIND-003-2 | `shapeChart`, timeSeries, barRows and explicit stat lookup reuse first-match `columnIndex`; table retains positional columns/cells. Validation and display therefore bind the same duplicate column. | CLOSED |
| FIND-003-3 | Chart signed bounds include zero and round outward; signed bar endpoints produce nonnegative heights and leave negative-label space. Shared component serves direct/view hosts. | CLOSED |
| FIND-003-4 | `MAX_SERIES=20`; folding starts above twenty, preserving nineteen plus aggregate. Four markers and five dash styles give twenty distinct pairs shared by plot and legend. | CLOSED |
| FIND-003-5 | CardHeader's named action states block pending/unavailable inputs, reset version selection to host-authoritative input and show ActionNote; WyrdView validates, deduplicates and reports host outcomes. | CLOSED |
| FIND-003-6 | Node checks abort after acquisition, installs active return/close listener, checks again after collect and removes listener. Native close notifies before waiting for the stream lock. WyrdView aborts and fences superseded controllers; Card listing checks signal before dependent pages. | CLOSED |
| FIND-003-7 | Rust-generated problem examples feed `wyrdProblem`; loader/view/MCP/action producers reuse canonical identities. Both endpoint refusal helpers serialize the complete problem; `failed` preserves denial. New range bypass is a separate producer failure, not a defect in this catalog projection. | CLOSED |
| FIND-003-8 | Contract's runtime JSON import uses standard `with { type: 'json' }`; loaders/MCP public entry imports remain in the root test script's native smoke. Browser SDK imports are type-only. | CLOSED |
| FIND-003-9 | Synthetic aggregate caption is checked against final retained authored captions before emission; renderer legend/tooltip use the same output. | CLOSED |
| FIND-003-10 | timeSeries keys typed `[raw group,y]` before summation, with distinct caption collision qualification. Null/text-null and multi-y identity remain separate. | CLOSED |
| FIND-003-11 | Native collects original Number as `Option<f64>`, takes the stream, then `parse_ceiling` uses decimal integer parse before shared `admit_ceilings`. Fractional/nonfinite/out-of-u32/negative originals cannot become a different admitted budget. Early refusal drops the owned stream. | CLOSED by R5 |
| FIND-003-12 | Shared `fmtCardRef` emits kind/name/version plus supplied optional UID; CardRefChip and CardHeader copy/display reuse it. Header's version-change reference intentionally omits stale UID. | CLOSED |

R5's correction sits at the true producer: both public SDK collection and Node executor cross the same native Number boundary, while MCP preserves its integer admission. Defaults and maxima still come from the shared owner, and actual overflow remains the existing 413. Native/server both reuse JsonResultBuilder for positional Arrow JSON/exact bytes while retaining their own existing terminal, clean-EOF, cancellation and settlement owners. General Arrow collection is not a replacement for this whole-JSON projection. Source plus the supplied native-backed refusal/closed-stream/fresh-query proof credibly closes the prior diagnosis.

## Narrow remediation verification recommendation

Use the existing selectors below after extending their assertions; these are implementation recommendations, not checks rerun by this validator:

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Validated portable rendering$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Dependent loads and safe actions$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^Canonical UI problem projections$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec node packages/ui/scripts/catalog.ts --check
mise run fmt:check
mise exec -- git diff --check
```

The existing `test`/`check` owners build the package/catalog. Never hand-edit generated output. Record meaningful RED/GREEN and actual selected counts; the native budget owner does not require another rebuild or live-server run for a pure range and rustdoc correction. Static inspection verifies FIND-003-14. No full journey, release, gate or unrelated aggregate is selected.

## Evidence limits

This validator performed read-only source/Git/authority inspection and wrote only this report. No test/build/generator or mounted-browser/endpoint execution is claimed. The focused follow-up separately records an artifact-free public-entry Node reproduction of schema acceptance, resolver RangeError and loadPanel rejection with zero executor calls. Those observations corroborate the path; source ordering establishes root-effect and endpoint propagation. Supplied candidate green checks remain reported evidence, including UI 40/40 and real-server 2/2; their existing cases do not cover this counterexample. Historical human/Darwin proof is not renewed Linux/image, live degraded-terminal or deployed-browser qualification.

Skipped: implementation and build/test reruns under the packet-only restriction. Risk: accepted unresolvable relative input still bypasses visible validation at the candidate; original numeric-budget loss is closed.
