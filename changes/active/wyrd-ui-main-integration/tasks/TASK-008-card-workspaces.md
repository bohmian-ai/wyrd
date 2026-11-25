# TASK-008 — Agent, Model and Data workspaces

Spec: [revision 28, approved](../spec.md). Status: Proposed; the existing safe requirement-detail prerequisite governs affected completion.
Dependencies: completed TASK-003, TASK-004 live shell/Observe and TASK-027; [TASK-005 Cards and Prompts](TASK-005-cards-and-prompts.md); [TASK-007 Verification](TASK-007-verification.md).
Mapped obligations: REQ-010/012/020/022/023/024/026/027/030/034/062; AC-007/009/010/011/013/016/017/019/041; INV-001–INV-007 apply, with installation-only obligations retained by their existing owners.
Absorbs former TASK-014/016/017, former TASK-005's CompositionGraph and ModelInterface, and former TASK-007's VerificationBindings. ArtifactTable is delivered with TASK-005's generic detail and reused here.

## Outcome and Value

A user opens an exact Agent, Model or Data version from Cards and sees its
declared definition beside genuinely attributed observations and recorded
verification. New CompositionGraph, ModelInterface and VerificationBindings
components enter the portable package and gallery together with these real
workspaces, so `mise run dev:full` exposes their actual first consumers.

## What Changes and What Must Stay True

Use TASK-005's `/t/[tenantKey]/cards/detail?kind=...&name=...&version=...`
route with optional `uid` and safely encoded query values; switching kind,
name or version changes the whole selected CardRef, never silently substitutes
latest. Register specialized workspace content on the same exact routes rather
than creating a second identity owner. Keep Files, Versions, YAML,
metadata and server-derived relationships, reusing CardHeader, CardFacts,
RelationshipList and ArtifactTable. Each kind has its own approved layout.

| Workspace | Required content and source | Truthful absence/negative behavior |
|---|---|---|
| Agent | Exact registered Prompt, execution route, runtime `tool_names`, `verified_by` bindings, composition, invocation/model-usage statistics and recent invocations. Count/latency/errors use actual `invoke_agent` spans; tokens/cost use actual gateway calls, attributed by this Agent's `subject_card_uid`. Recent invocations link to exact traces. | Missing attribution explains which observation would fill it. Resource labels, shared account, workload alone and assumed `agent.invoke`/`prompt.render` spans do not create Agent activity. |
| Model | Declared interface/framework metadata, task type, signature input/output order, optional sample-input description, artifacts and exact relationships. Actual attributed spans/metrics group by their returned names; recorded drift comes from bound generic Verifiers. | No `model.predict` assumption or telemetry filled from defaults. Absent observations, no bound drift and denied/failed reads remain different states. |
| Data | Declared interface/schema, `stats`, `splits`, `target_columns`, named/default `sql` and artifact references; recorded per-feature drift and score history. Find drift-check Verifier declarations whose exact `baseline_ref` names this Data Card and preserve Verifier/requirement/result/window selection. | Do not infer baseline distributions, use `split` instead of `splits`, join only by a name or invent a Data telemetry model. No matching baseline is an informative empty state. |

Own CompositionGraph, ModelInterface and VerificationBindings with their typed
props, portable loaders, catalog/view entries and JSON-safe action intents.
Graph edges come from the exact Agent/Service declarations and exact references;
node badges project recorded results only. Agent is the first conventional
CompositionGraph consumer; Service later reuses the same component. Bindings
show declared Verifiers and recorded status/results, preserving binding,
subject, workload, execution and application Run identities; a binding without
a result is not a pass. The Model and Data pages reuse the drift component
delivered by TASK-007 rather than implementing their own scoring.

Components never fetch or hold credentials; shared loaders own domain reads,
dependent-read sequencing, SQL and response shaping; Node/MCP executors own IO.
Pages own only layout, URL/selection/range and action-host state. Preserve current
permissions/audit, null managed attribution and stable Wyrd errors. Denied or
failed prerequisites stop dependent reads. Input/version/range/navigation changes
supersede old loads even when transport cancellation cannot stop remote IO.

**External completion limits.** Reuse TASK-005's Files tab and existing registered
file inventory on each kind-specific page. Per-feature drift and
requirement detail now live in sensitive `execution_outputs.output`, not the
retired `bifrost.drift.result_features` table. TASK-007's separately approved
non-sensitive read prerequisite must supply required detail consistently in
Node/MCP before these regions can close. Independent declared-content and
attributed-observation work may proceed, but fixtures/unavailable actions cannot
complete affected live outcomes. No new safe-detail contract or security grant
is authorized by this task.

## Approved Canvases

These approved canvases define the visual target for this task. Follow their layout, component placement, hierarchy and interactions while using current Wyrd contracts and brand tokens. Apply the same component design in the gallery and the working UI. Record a separate comparison and human acceptance for each named canvas on desktop/mobile in light/dark.

- [Agent.dc.html](../references/canvas/Agent.dc.html)
- [Model.dc.html](../references/canvas/Model.dc.html)
- [Data.dc.html](../references/canvas/Data.dc.html)

If a required visual detail is absent or contradictory in these approved canvases, stop that affected visual work and ask the maintainer for clarification. Do not guess a layout, invent a replacement design, or treat a generic directory link as a visual specification.

## Approach

1. Confirm current generated Agent/Model/Data/ref/binding contracts and the
   exact Card route/loader owners delivered by TASK-005/007.
2. Implement each new portable component with its first workspace consumer,
   catalog/gallery entries and actual Node/MCP projections.
3. Compose each kind-specific board from shared Card/Observe/Verify components,
   preserving exact identity and truthful absent/denied/error behavior.
4. Supply representative real versions, artifacts, attribution and drift through
   the existing normal development setup and public SDK workflows.
5. Prove cross-host reads and authenticated navigation, then inspect Agent,
   Model and Data independently alongside each new gallery component.

## Ordered Implementation Scenarios

### Scenario 1 — Agent declaration and attributable activity

**Behavior.** Opening an exact Agent renders its Prompt, route, runtime tools,
composition and bindings, then the actual activity attributed to its UID.
Another release sharing a Prompt, workload or account does not contribute
activity without the selected subject attribution. A graph node navigates to
its exact Card; a recent invocation opens its exact trace. Badge/status comes
only from recorded results. No observations, no results and denied observations
each explain their own state.

**RED.** Add planned `agent workspace preserves declared and observed identity`
([V1](#v1--agent-projection)). Assert exact composition/reference links, current
Agent fields, binding/result separation and actual subject-scoped request/binds.
Exercise all new component states and superseded-version/range responses.
The absent projections/components fail; do not assert board example numbers.

**GREEN.** Implement portable projections and mount the Agent consumer in the
existing Card host. Reuse existing StatStrip/Chart/RecordTable and exact Observe
links. Rerun V1 without broadening generic telemetry naming rules.

**REFACTOR.** Remove duplicate page reads/shaping in favor of the shared owners
while preserving the Agent-specific source semantics and V1 assertions.

### Scenario 2 — Model definition, artifacts and recorded signals

**Behavior.** Opening a Model renders the exact declared interface, ordered
signature, optional sample-input metadata and artifacts. Observations show only
returned attributed signal names; bound-Verifier drift preserves recorded
feature/result values. Artifact navigation/actions use TASK-005's actual
supported host capability and expose pending/success/denied/error states. No
artifact or no observation is not an IO failure.

**RED.** Add planned `model workspace preserves interface and recorded signals`
([V2](#v2--model-projection)). Assert generated discriminated interface fields,
signature order, exact artifact reference and absence-versus-denial, including
unattributed rows and two exact versions. Missing artifact projections remain a
dependency, not a fabricated success assertion.

**GREEN.** Deliver ModelInterface and the Model route together, reusing
ArtifactTable, bindings and drift rather than replacing their contracts. Rerun
V1/V2 and retain blocked action evidence until its platform prerequisite ships.

**REFACTOR.** Share existing Card/action state owners and eliminate local casts
or hardcoded inference-event assumptions while both projections stay green.

### Scenario 3 — Data definition and exact baseline drift

**Behavior.** Opening Data shows current serialized schema/stats/splits/targets
and SQL without inventing fields. Recorded drift is discovered through a drift
Verifier check's exact baseline_ref and scoped by its exact requirement and
selected result/window. Two versions with the same name do not share baselines.
Score history uses recorded scores/thresholds only; denied/failed detail stays
visible and never turns into no drift or a drawn baseline distribution.

**RED.** Add planned `data workspace preserves exact baseline drift`
([V3](#v3--data-projection)). Assert ordered schema/current field names,
baseline-version matching, per-requirement/result/window selection, no-baseline
empty state and denied prerequisite with no dependent reads. Against approved
safe-detail fixtures, prove all features remain visible and recorded verdicts
are unchanged.

**GREEN.** Compose declared facts and TASK-007's authorized drift projections
through portable loaders; revalidate current public reads before any contract
extension. Rerun V1–V3; absent detail contracts cannot be patched with SQL over
retired or sensitive sources.

**REFACTOR.** Keep a single recorded drift interpretation and remove page-level
joins while exact baseline and authorization assertions remain green.

### Scenario 4 — Portable package and real workspace journeys

**Behavior.** Every owned component validates/renders in catalog/gallery and the
built MCP artifact and reads real authorized data through Node/MCP. A normal
human starts `dev:full`, signs in, uses Cards to open each kind, switches versions
and ranges, follows relationships/invocations and inspects bindings/drift. Empty,
denied/error and foreign-tenant cases remain truthful; stale responses cannot
replace a newly selected version or window.

**RED.** Add planned `authorized card workspaces across hosts`
([V4](#v4--real-authorized-hosts)) and `authenticated card workspaces`
([V5](#v5--real-browser-journey)). The first asserts actual requests/results,
built AppBridge rendering and T5 populated/empty/denied cases; the second uses
the existing Chromium/production-BFF/public-human-login owner to exercise all
three actual pages and visible interactions. A fixture-only component gallery
or page-handler import cannot satisfy either live proof.

**GREEN.** Close routes, host actions and missing representative development
data through the existing lifecycle/public workflows. Record exact seeded
Cards/versions, attributed observations and result/baseline identities, and
separate expected legitimate empty states. Rerun V1–V5 and scoped checks.

**REFACTOR.** Reuse one package, session BFF, browser harness and development
launcher; private fixtures/layout/helper placement remain implementation-owned.

## Acceptance Criteria

| Obligations | Required observation and proof |
|---|---|
| REQ-010/012/024; AC-007 | V1–V3 and each live workspace show the complete named Agent, Model and Data board regions from their exact current declarations. No stale workspace layout/type or invented event/value supplies content. |
| REQ-022/023; AC-016/017 | V1/V2/V4 isolate selected subject and exact workload/version from shared labels/accounts/components; null attribution stays null and grants no authority. |
| REQ-026/027; AC-009/019 | V1–V4 preserve declared bindings, exact selected results and baseline/Verifier/requirement/window identities, recording no judgment locally. Missing safe-detail/artifact projections keep affected outcomes incomplete. |
| REQ-030; AC-011; INV-001–007 | CompositionGraph, ModelInterface and VerificationBindings each have T1 all prop states, T2 actual requests/shapes, T3 validated views, T4 built AppBridge rendering and applicable real authorized T5 proof. Existing shared components/actions retain regressions. |
| REQ-034/062; AC-013/041 | V5 plus normal `dev:full` proves reachable exact Card workspaces, useful seeded populated states, range/version/action behavior and denied/error/empty/cross-tenant cases, with no credential serialization or fixture fallback. |
| REQ-020; AC-010 | Individually inspect the three new gallery components and Agent, Model and Data boards: desktop/mobile, light/dark, keyboard/focus, touch, long graph/signature/table content and overflow. Record board region/screenshot/state/theme/viewport and human acceptance separately. |

## Implementation Reference

Source locations guide existing owners and consumer closure, not an implementation
allowlist or fixed private module design.

- **Exact Cards:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts::Cards::{get,list,listArtifacts}`
  and package `loaders.ts::loadCardHeader` already read exact envelopes, artifact
  inventory and version lists. Inspected test owner:
  `sdks/wyrd-sdk-ts/wyrd/tests/support/server.ts::{declared,fixture}`. Extend
  TASK-005's route/projections, not another registry/artifact/lineage owner.
- **Declared content:** current `AgentSpec::{prompt,execution_route,tool_names,verified_by}`,
  `ModelSpec::{interface,task_type,signature,sample_input,card_refs}` and
  `DataSpec::{interface,schema,card_refs,splits,target_columns,sql,stats}` live in
  `crates/wyrd-spec/src/card/{agent,model,data}.rs` and generated
  `sdks/wyrd-sdk-ts/wyrd/src/card-types.ts`. Their current serialization/validation
  tests fix the input forms; no historical StoredModel/Data/Agent projection is
  reused. Add missing generated wire roots through the existing generator.
- **Verification:** `VerifierSpec`, `VerificationBinding` and drift check
  declarations in `crates/wyrd-spec/src/card/verifier/mod.rs`, generated
  `DriftSignalDistribution.baseline_ref`, SDK `verify.Execution.status`, and current
  `ResultsTable`/`ExecutionOutputsTable` are the owners of declaration,
  selected-result and sensitive-detail semantics. Inspected consumer:
  `sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time.test.ts`; reuse
  TASK-007's safe projection and do not query the removed result-features table.
- **Portable reads/hosts:** package `loaders.ts::loadPanel`,
  `node.ts::nodeExecutor`, `mcp.ts::mcpExecutor`, schemas/catalog generation and
  `src/routes/t/[tenantKey]/ui/load/+server.ts::POST` already separate projection
  from same-viewer IO. Existing Observe server-load and action consumers show
  exact navigation/range cancellation. Extend these owners and the existing
  catalog/action contracts, never import a Node SDK into a browser component.
- **Proof/setup:** `ui-library.test.ts`,
  `ui-library.integration.test.ts::{appBridgeView,mcpOver}`, and
  `ui-pages.integration.test.ts::{startBff,humanSignIn}` are the current renderer,
  real SDK/MCP and Chromium/BFF owners. `mise.toml` and
  `scripts/dev/example-app.sh` own the canonical normal workflow. Seed missing
  public examples there without a second launcher or writes to protected tables.

## Verification and Evidence

V1–V5 are **planned** titles in the existing Vitest files; confirm each actual
selector after adding it and before RED. The existing package script includes
the public-package build before Vitest; current Vite includes these files and
gates integration with `WYRD_UI_INTEGRATION`. Skips/no selected tests are not
proof. Each selected test must exercise the assertions named in its scenario.

### V1 — Agent projection

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^agent workspace preserves declared and observed identity$'
```

### V2 — Model projection

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^model workspace preserves interface and recorded signals$'
```

### V3 — Data projection

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^data workspace preserves exact baseline drift$'
```

### V4 — Real authorized hosts

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^authorized card workspaces across hosts$"'
```

### V5 — Real browser journey

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^authenticated card workspaces$"'
```

Before V4/V5 use the existing [managed setup](../references/verification.md):
`mise run ts:build`, `mise run ts:build:testing`, `mise run build:server`, the
existing workspace/test-browser installation and current production UI/library
outputs. Use the existing server/Postgres and production BFF lifecycle; do not
add a selector API, runner or production switch. After focused tests:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run codegen:check
```

Manual checkpoint: run `mise run dev:full`, sign in with the printed local
credentials, navigate Cards → Agent/Model/Data exact versions, and inspect
`/_gallery` for each new component. Record representative populated and
legitimate empty seeds, actual actions/links/ranges, and source/browser identity
with sanitized requests/screenshots/traces and per-component/page human
acceptance. Recheck `dev:batch` if setup changes. Artifact T4/host parity is part
of this task; production MCP registration/live conversation remains TASK-012.

## Material Stop Conditions

Stop affected work when the required artifact action or safe detail contract is
absent, or current source requires a new public/persisted/security/ownership
decision. Finish independent declared-content/observation scope and identify the
precise blocked consumer; do not guess wire payloads, reclassify sensitive output
or bypass missing action projections. Seeding that requires an unavailable public
capability is a platform prerequisite, not permission to inject protected rows.
Ordinary private helpers/layout/fixture structure are reversible implementation
choices. Missing cross-platform/browser/human evidence stays unverified.

## Authority Links

[Approved spec](../spec.md), [delivery and obligation map](../plan.md),
[approved canvases](#approved-canvases), [reference audit](../references/task-audit-2026-10-29.md),
[AGENTS.md](../../../../AGENTS.md), [execution](../../../../changes/implementation-execution.md),
[TDD](../../../../changes/spec-driven-development.md), [testing](../../../../TESTING.md),
[Wyrd design](../../../../architecture/wyrd-design.md),
[security](../../../../architecture/wyrd-security-posture.md).
