# TASK-027 immutable cumulative review subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; branch `wyrd-ui-main-integration`.
- Base: `5e57416d562386066c1aadda9dd01783b9cd4eb5` (rebased equivalent of original `a1e9b48d2`).
- Candidate: `7579a2f603c023daae0d15b9d5cc3013eec6f00d`.
- Approved authority: `changes/active/wyrd-ui-main-integration/spec.md`, revision 13. Revision 13 only removes `space` from exact Card reference following flat-identity rebase.
- Original task: `changes/active/wyrd-ui-main-integration/tasks/TASK-027-view-layout.md`.
- Prior packets: `review/task-027-ab1d2499f-1/`, `review/task-027-c2bbd8e47-1/`, `review/task-027-a0a5f177f-1/` relative to the change packet. Inspect verdicts, validation ledgers, R1/R2/R3 tasks as hypotheses, never authority overriding source.
- Latest remediation: TASK-027-R3, FIND-027-5 reactive authored document reset; candidate commit `7579a2f60`.

## Cumulative attribution and rebase

Read the actual complete range with `git diff 5e57416d5...7579a2f60`. TASK-027 starts at `bc4b73011`, example header sizing `7c7a7dead`; R1 `1a4a9bbe9`, `2e528db14`; R2 `e31b220f8`, `2bcf10923`, test typing `a55a500ce`; R3 `7579a2f60`. Other cumulative commits and files include TASK-003 work and packet updates: inspect seams but do not report independent TASK-003 defects unless layout breaks. In particular `4eca1e2ba`, `bde4396d9`, `f0fff4b13` are TASK-003. Unrelated PR #205/main changes are excluded.

Orchestrator ran `git range-diff a1e9b48d2..wyrd-ui-pre-205 5e57416d5..7579a2f60`. All reviewed implementation commits map with `=` including initial layout, R1, R2, tests and prior packets. Only mapped `5d27f6447 ! e4bc00a0f` differs in packet authority link paths; no implementation conflict. Ten old upstream commits appear only on old side because the comparison begins at the historical task base; PR #205/main is not review scope. Post-rebase authority links, revision 13, TASK-003 fixes and R3 appear only on new side.

## Shared starting navigation map (expand independently)

UI prefix `crates/wyrd/wyrd-server/wyrd-ui/`:
- `packages/ui/src/lib/schemas.ts`: LayoutSchema / PanelSchema / ViewDocumentSchema; `scripts/catalog.ts` publishes view schema in generated catalog, component schemas remain size agnostic.
- `packages/ui/src/lib/layout.ts`: grid constants, rowsHeight/rowsFor, place collision/auto occupancy, edit snapping/bounds.
- `packages/ui/src/lib/WyrdView.svelte`: parsed/content/source/loader; authored + given reset effect; kept/draft/release; placements/ordered/boxStyle/current/commit/cancel/key/drag; measured panel heights, handles, narrow and fill styles. Root owner of local layout and gesture lifecycle.
- `packages/ui/src/lib/view.ts`: public WyrdView mounting and onlayout host projection; `packages/ui/src/app/main.ts`: MCP per-render host lifecycle.
- Gallery `src/routes/_gallery/+page.svelte`, fixtures and `examples/ui-app/src/panels.ts`, `src/routes/view/+page.svelte`: direct host callers and responsive examples.
- `src/lib/ui-library.test.ts`: four original TASK-027 scenarios plus pointer cancellation and arbitrary panel IDs; reactive-host additions in layout editing/pointer cancellation. `src/lib/ui-fixtures.svelte.ts` creates native Svelte proxy for tests; vitest setup supplies measurements/pointer facilities.
- Chart resize/fill and loader cancellation are cross-task seams; enumerate sibling consumers and existing owners before reporting duplication.

## Authorities and coverage

Read root AGENTS.md, changes/spec-driven-development.md, agent_docs/maintainer-style.md, agent_docs/index.md, applicable topic guides, architecture/README.md and owning design sections, TESTING.md, UI brand/DESIGN.md and relevant UI skill references. No nested AGENTS.md exists in changed UI/example paths; SQL AGENTS.md is outside changed scope. No root `.codegraph/` exists, so skip CodeGraph. Read only applicable architecture sections. Sensitive domain: browser interaction lifecycle/accessibility and reactive state ownership, assigned to domain-review-interaction.md. No material security, tenancy, server persistence or database boundary is changed by attributed TASK-027.

## Available verification

User reports candidate-integrated green: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` (package and app, zero errors), `test` 40/40, `build`; `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts"` 2/2. Treat these as supplied evidence, distinguish freshly inspected source and historical browser/human acceptance. Prior packet verification contains browser geometry/focus/capture and human checkpoint evidence. Avoid repeating integrated lanes without a new source concern.

## Review constraints

Only write your assigned new report in this directory. Do not change source, tests, existing packets, git metadata, commit or push. Source is immutable. Commands through mise where applicable; avoid build/check commands that rewrite generated outputs. Review all cumulative obligations, close FIND-027-1..5 from source, apply the skill’s exact role requirements, no optional improvements. Check HEAD remains candidate. Report missing evidence honestly. Reports only; no diff/patch/archive artifacts. Each discovery reviewer is independent and must not read other current-round reports.
