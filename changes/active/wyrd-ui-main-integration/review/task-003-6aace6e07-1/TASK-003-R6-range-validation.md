---
id: TASK-003-R6
kind: remediation
status: ready
spec: SPEC-wyrd-ui-main-integration
spec_revision: 13
parent_task: TASK-003
remediates: [FIND-003-13, FIND-003-14]
---

# Reject unresolvable ranges through the existing validation surface

Route directly to `$wyrd-implement`. Finish shared relative-range validation so accepted input either resolves to valid absolute bounds or returns the existing field-specific validation problem. Batch the small mandatory test rustdoc correction into this task.

## Subject and authority

- Approved specification: `changes/active/wyrd-ui-main-integration/spec.md`, revision 13, especially REQ-030 validated documents/loaders and visible panel errors.
- Original task: `changes/active/wyrd-ui-main-integration/tasks/TASK-003-shared-ui-foundation.md`, scenario 1 and visible Chart schema-error acceptance.
- Reviewed candidate: `6aace6e07728e59deb1fcd85ce969b2d0a4260db`.
- Cumulative base: `c62ebce8c4de0d43a2b9d50d360de628c49fd533`, parent of `ff4145ac6`.
- Independent diagnosis, caller trace and selected correction: [findings-validation.md](findings-validation.md); focused reachability evidence: [followup-review.md](followup-review.md); disposition: [verdict.md](verdict.md).

Follow root/nested AGENTS, the shared UI guidance and existing implementation workflow. FIND-003-1..12 are closed, including R5; preserve their corrected behavior. Unrelated PR #205/main and TASK-027 layout are not remediation scope.

## Diagnosis and selected correction

### FIND-003-13 — Syntactically admitted ranges can escape error handling

The shared relative schema accepts `{last:"1000000000000d"}`: its only value check is positive digits plus a supported unit. Both root view documents and explicit Chart inputs use this schema. At `packages/ui/src/lib/loaders.ts:75–78`, resolution subtracts the multiplied duration and calls `toISOString()` on an invalid Date. The original foundation commit already contains this path; it is not TASK-027 work.

UI paths here are rooted at `crates/wyrd/wyrd-server/wyrd-ui/`. In `packages/ui/src/lib/WyrdView.svelte:240–256`, root range resolution happens after old controllers are aborted but before panel scheduling and promise rejection handlers. The accepted document therefore throws without its visible invalid-document state, and no panel loads. For a Chart's explicit range, `loadChart`/`loadPanel` reject before an executor call: WyrdView converts that into an internal panel error, while the shipped `src/routes/t/[tenantKey]/ui/load/+server.ts` and custom `examples/ui-app/src/routes/view/load/+server.ts` directly await the rejected loader and bypass their honest error representation. Validating a separate inherited absolute range at those endpoints does not validate the explicit Chart range.

This violates the existing validation/error contract. It does not justify imposing a retention window or requiring valid siblings to execute under an invalid root document. An invalid root range needs a visible document rejection with no reads; an invalid panel range must fail only that panel.

Extend the existing shared range schema/resolution authority using native Date validity and the existing absolute-range contract. Before ISO conversion can escape, resolution must yield valid absolute bounds or the existing canonical validation problem naming the failed range field. Reuse that result through existing document and Chart validation/error states, so both endpoint consumers resolve to their existing problem/loaded-error representation rather than reject. The source owner must prevent the invalid state; repeated numeric guards at individual hosts would leave other callers vulnerable.

Existing mechanisms suffice: `RangeSchema`, `resolveRange`, `inputProblem`/catalog-derived `wyrdProblem`, Loaded error props and renderer validation display. There is no need for a new parser/service, dependency, duration setting or error family. Preserve the direct custom-app page's authored `24h` range as a working consumer if internal resolution changes. Private signatures and control flow remain implementation-owned.

### FIND-003-14 — Required panic documentation is incomplete

`crates/shared/wyrd-client/src/client.rs:760–774`, `browser_session_bifrost_connects_without_an_ingest_channel`, is newly added by the cumulative task. Its descriptive rustdoc lacks `# Panics` despite its session/client/connection `expect` assertions. Root AGENTS and `agent_docs/rust.md` require that content for changed tests. The existing prose does not satisfy the explicit documentation gate.

Complete this test's existing rustdoc with the assertion panic conditions, following the nearby documented test convention. Preserve the test body and assertions. Static inspection and format checking are sufficient; add no runtime proof or separate documentation-only round.

## Preserved behavior and scope

Keep one shared resolution clock per inherited view load, valid ordinary relative/absolute ranges, explicit panel-range precedence, exact authored SQL and typed binds, supported cancellation and supersession, independent panel errors, credentials/tenancy and all TASK-027 layout behavior. Native Number admission, query ceilings, output accounting, terminal/settlement ownership, transport and session authority remain as currently corrected.

Expected scope is the existing shared range admission/resolution and directly affected renderer/loader/example consumers and focused assertions, plus the one test rustdoc block. Regenerate catalog projections through the existing generator if their source changes; never hand-edit generated artifacts. No arbitrary duration/retention cap, repeated per-host arithmetic validation, blanket exception relabeling, public contract expansion, new harness/configuration, query/native changes, Ask, saved views, A2UI or publishing is authorized. A new material decision returns to specification authority.

## Acceptance and focused proof

Extend the existing pure foundation rendering/load/error assertions rather than adding a new framework or live-server prerequisite.

| Finding / scenario | Required observation |
|---|---|
| FIND-003-13, invalid root range | The exact counterexample produces a visible canonical field-specific validation error and zero executor reads, with no escaped effect error. Cover replacing a previously valid document as well as initial input. |
| FIND-003-13, invalid explicit Chart range | With a valid inherited range and a valid sibling, the invalid Chart shows canonical validation identity/field while its sibling loads; no query is issued for the invalid input. |
| FIND-003-13, both HTTP hosts | Exercise the existing imported BFF and custom endpoint harness with valid inherited bounds and the counterexample on the Chart. Each handler resolves to its existing honest error representation with validation identity/field, rather than rejecting; no executor read occurs. Reuse current test/runtime setup for any normal client construction. |
| FIND-003-13, regression boundary | Ordinary fixed-clock/six-hour and absolute ranges, explicit precedence, exact binds and cancellation assertions remain green; the direct example's `24h` range remains valid. |
| FIND-003-14 | The existing test rustdoc documents its panic conditions and the test body/behavior is unchanged. Static inspection suffices. |

Record meaningful RED/GREEN and actual selected counts for the added range cases. The report-only review's Node assertion is corroborating evidence, not a substitute for implementation proof. Run the narrow existing owners sequentially:

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Validated portable rendering$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Dependent loads and safe actions$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^Canonical UI problem projections$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec node packages/ui/scripts/catalog.ts --check
mise run fmt:check
mise exec -- git diff --check
```

The existing test/check tasks rebuild the package. This pure range and documentation correction needs no native rebuild, live-server suite, release matrix or broad aggregate. Final change review owns combined journey proof. A subsequent task review reassesses the complete cumulative TASK-003 candidate.

## Implementation evidence

Commit `6eb137e7b`. Status: **IMPLEMENTED**.

Owner and gap: `RelativeRangeSchema` (`packages/ui/src/lib/schemas.ts`) is the single admission point for root and Chart ranges, and every consumer (WyrdView document parse, `validatePanel`/`loadPanel`, both POST endpoints) already maps its failure to `inputProblem` → canonical `validation`. The gap was that the schema admitted spans `resolveRange` could not convert. The `last` field now refines with native `Date` validity of the span measured back from epoch 0, so an admitted span resolves for any clock at or after 1970. `resolveRange` moved beside the schema (still exported through `wyrd-ai-ui/loaders`, public surface unchanged) so admission and resolution share one span computation. No host, endpoint, renderer or example code changed; the catalog JSON is unchanged (`--check` green).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-003-13 invalid root range: visible canonical field error, zero reads, initial and replacing a valid document | `schemas.ts` `RelativeRangeSchema` refine; existing WyrdView `!parsed.success` EmptyState | `TASK-003: Validated portable rendering` new block: `View input.range` + `WYRD_SPEC_400_VALIDATION`, call count stays 1 after valid→invalid | PASS |
| FIND-003-13 invalid explicit Chart range: field error, sibling loads, no query | same refine via `validatePanel` | same block: `far` shows `Chart input.range` + code, `near` renders legend, 2 total reads (only valid panels) | PASS |
| FIND-003-13 both HTTP hosts refuse rather than reject | same refine via `loadPanel` | `Canonical UI problem projections` new loop: BFF and example POST answer 400 `application/problem+json` `{...validation, detail: /^Chart input\.range/}`; `Bifrost.connect` spy never called | PASS |
| FIND-003-13 regression boundary | `resolveRange` arithmetic unchanged | six-hour/fixed-clock, absolute, explicit precedence, binds and cancellation assertions in `TASK-003: Dependent loads and safe actions` green; `100000000d` boundary still admitted; example `24h` unaffected; full UI suite 40/40 | PASS |
| FIND-003-14 `# Panics` rustdoc | `crates/shared/wyrd-client/src/client.rs` test rustdoc; body unchanged | static inspection; `fmt:check`, `lints`, focused nextest green | PASS |

RED (before fix): `Validated portable rendering` 1 failed (`RangeError: Invalid time value in $effect in WyrdView.svelte`); `Canonical UI problem projections` 1 failed (`RangeError` escaping `POST src/routes/t/[tenantKey]/ui/load/+server.ts:24`). GREEN after fix; each selector selects 1 test (9 skipped).

Commands (all PASS):

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Validated portable rendering$'   # 1 passed
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Dependent loads and safe actions$' # 1 passed
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^Canonical UI problem projections$'          # 1 passed
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test     # 8 files, 40 passed
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check    # 0 errors, 0 warnings
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec node packages/ui/scripts/catalog.ts --check
mise run fmt:check
mise run lints
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=client::tests::browser_session_bifrost_connects_without_an_ingest_channel)'  # 1 passed
mise exec -- git diff --check
```

Non-goals held: no duration/retention cap beyond `Date` representability, no per-host checks, no new error family, parser, configuration, dependency, native/query change or public export. Real-server UI lane not rerun (pure TS range validation plus rustdoc; no native or server change). Changed files: `schemas.ts`, `loaders.ts`, `ui-library.test.ts`, `client.rs` only.
