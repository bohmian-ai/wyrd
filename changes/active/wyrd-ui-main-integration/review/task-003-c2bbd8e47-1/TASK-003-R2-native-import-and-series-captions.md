---
id: TASK-003-R2
kind: remediation
status: ready
spec: SPEC-wyrd-ui-main-integration
spec_revision: 12
parent_task: TASK-003
remediates: [FIND-003-8, FIND-003-9]
---

# Restore native public imports and distinguish folded series

Execute with `$wyrd-implement` under approved revision 12.
Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
Specification: `changes/active/wyrd-ui-main-integration/spec.md` revision 12 approved.
Original task: `changes/active/wyrd-ui-main-integration/tasks/TASK-003-shared-ui-foundation.md`.
Original base: `695ca914df61a8683059f6f32279ec2148424e06`.
Reviewed cumulative candidate: `c2bbd8e47e4752b3772ad75acfaf23f3400ac704`.
Previous candidate: `ab1d2499f6a08a6b5afe1f6c904e72746d526bda`; prior R1 and closed FIND-003-1..7 reside in `../task-003-ab1d2499f-1/`.
Current verdict and independent diagnosis: `verdict.md`, `findings-validation.md`, and caption correction follow-up `followup-review.md` beside this task.

UI-relative paths below start at `crates/wyrd/wyrd-server/wyrd-ui`.

## FIND-003-8 — native Node cannot link public exports

Obligation: original TASK-003 export/package acceptance and REQ-030 portable loaders on a user's Node server.

**Diagnosis.** `packages/ui/src/lib/contract.ts:19` imports generated problems.json without an ESM JSON import attribute. svelte-package emits the same ordinary import in dist/contract.js, package.json declares ESM, and public `wyrd-ai-ui/loaders` and `/mcp` both runtime-import this contract. On pinned Node 26.2.0 both public imports independently fail with ERR_IMPORT_ATTRIBUTE_MISSING before any read executes. This is valid package resolution followed by invalid module linking, not the example's cwd/workspace issue. Vitest/Vite transform JSON and therefore pass while masking native linking. The canonical metadata generator/distribution and prior FIND-003-7 correction are otherwise correct.

**Selected correction.** Use the standard `with { type: 'json' }` attribute at the existing runtime metadata import. Keep generated problems.json, its owning catalog generator, canonical helper and browser-safe native-free graph. This one import serves both failing public exports. Do not replace JSON with a handwritten catalog, filesystem read, custom loader, bundling layer, dependency or Node-only error helper; no new public or server contract is needed.

**Acceptance/proof.** After the existing package build, plain pinned Node imports both public exports and executes a catalog-backed validation path. Preserve canonical browser errors, upstream denial identities and browser/AppBridge compatibility. Add a compact native smoke assertion to an existing package verification owner; another Vitest-only import cannot prove this boundary. No new test harness or SDK native rebuild is required.

Exact direct smoke command, run with cwd `crates/wyrd/wyrd-server/wyrd-ui`:

```sh
mise exec -- node --input-type=module -e 'import assert from "node:assert/strict"; const loaders = await import("wyrd-ai-ui/loaders"); const mcp = await import("wyrd-ai-ui/mcp"); assert.equal(loaders.chartParams("SELECT $1", undefined).code, "WYRD_SPEC_400_VALIDATION"); assert.equal(typeof mcp.mcpExecutor, "function");'
```

Run the command directly as shown so the shell does not expand `$1` in the assertion. First run `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui package` from repository root to produce package contents. Record the actual Node command/cwd and output.

## FIND-003-9 — real series named other collides with synthetic aggregate

Obligation: REQ-025 allows arbitrary text series and folds overflow into other; REQ-030 requires complete truthful charts through every host.

**Diagnosis.** `packages/ui/src/lib/plot.ts:48-54` keeps nineteen ranked authored groups and appends synthetic `other`. A high-total real `other` remains, yielding two same-caption entries. Independent source-aligned data probe using s0..s19 values 1..20 plus other=21 returns twenty entries, nineteen unique captions, real other=21 and aggregate other=3. Chart at `components/Chart.svelte:209,233,256,263,270` keys plot/tooltip/legend by caption and also uses it in accessible text. Duplicate keys fail Svelte reconciliation; removing keys alone would still display indistinguishable tooltip `other: 21` and `other: 3`. Existing numeric-name 5/20/21 tests miss this valid text input. The early-fold threshold finding FIND-003-4 is closed; this is a distinct producer cause.

**Selected correction.** Qualify the synthetic aggregate caption in the existing timeSeries overflow producer when necessary so it remains recognizably the other aggregate and differs from all retained authored captions. Reuse the existing private Series.label; preserve authored labels/points, deterministic ranking, nineteen-plus-one output, sum per instant, ordinary non-colliding other caption, and all at-or-below-twenty behavior. Any alternate fixed spelling is also legal authored input, so collision avoidance must cover it too; local suffix mechanics remain implementation-owned.

This producer correction fixes keyed identity and attribution across every sibling consumer. No new public identity/flag/schema, renderer normalization, host guard, dependency or reserved series value is needed. Do not hide a line, merge the retained real group into the aggregate, reject a valid caption, or only switch loops to positional keys. No requirement for literal identical captions supersedes truthful attribution.

**Acceptance/proof.** Extend the existing portable rendering scenario with twenty-one groups including retained real other. Assert twenty plot/legend entries, real value 21 unchanged, aggregate sum 3 under its distinct recognizable caption, no duplicate-key error, distinct tooltip and accessible attribution. Include authored text matching the chosen qualification to catch merely substituting another constant. Rerender changed ranking/cardinality and verify current values/captions/cues. Retain existing 5/20/21 and fold-total checks. No live server is needed for this pure producer-to-component path.

## Preserved scope and narrow verification

Only the existing package contract import, private aggregation producer, directly affected focused tests and existing package verification wiring are necessary. Preserve prior FIND-003-1..7 closure, exact SQL/binds, positional results, signed geometry, action states, cancellation, canonical metadata, tenant/auth/audit owners, all hosts and separate TASK-027 layout work. No dependencies, APIs, persistence, Ask/A2UI, production MCP registration, npm publication, general cleanup or new generated catalog.

Apply Red/Green to the exact failure paths; private fixture/control-flow mechanics remain implementer-owned. In addition to the plain Node smoke command above, run:

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Validated portable rendering$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^Canonical UI problem projections$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise exec -- git diff --check
```

Run codegen:check only if generated inputs/wiring actually change; neither selected correction requires that. No broad aggregates, Linux release bundles or full user-journey suites belong to this remediation. Outstanding Linux/live degraded proof stays unverified at its owning stage. Stop for any genuinely new product/API/security/durable decision rather than expanding scope. Later review reassesses the complete cumulative original task and both R1/R2, preserving stable FIND IDs.
