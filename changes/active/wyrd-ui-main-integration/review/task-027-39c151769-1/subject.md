# TASK-027 immutable cumulative R4 re-review

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; branch `wyrd-ui-main-integration`.
- Cumulative base: `5e57416d562386066c1aadda9dd01783b9cd4eb5` (rebased equivalent of original `a1e9b48d2`).
- Immutable candidate: `39c15176955edca785cc578168c67e1efcb80ea5`.
- Approved specification: `changes/active/wyrd-ui-main-integration/spec.md`, revision 13.
- Original task: `changes/active/wyrd-ui-main-integration/tasks/TASK-027-view-layout.md`; its revision-12 layout obligations are unchanged by revision 13.
- Read actual cumulative diff: `git diff 5e57416d5...39c151769`. Latest remediation diff is a locator, never the complete acceptance subject.

## History and attribution

Prior packets under this change's `review/`: `task-027-ab1d2499f-1/`, `task-027-c2bbd8e47-1/`, `task-027-a0a5f177f-1/`, `task-027-7579a2f60-1/`. Read verdicts, validation ledgers and R1/R2/R3/R4 remediation tasks as hypotheses; verify FIND-027-1..6 closure against current source. Latest remediation is `task-027-7579a2f60-1/TASK-027-R4-narrow-document-replacement.md`. Commits `3302a3e6b` change narrow projection and the existing narrow scenario; `39c151769` records evidence only.

TASK-027 implementation starts at `bc4b73011`, example sizing `7c7a7dead`; R1 `1a4a9bbe9`, `2e528db14`; R2 `e31b220f8`, `2bcf10923`, test typing `a55a500ce`; R3 `7579a2f60`; R4 above. The complete cumulative range also contains independent TASK-003 work and packet changes. TASK-003 commits `4eca1e2ba`, `bde4396d9`, `f0fff4b13`, `18e62740f`, `791122315` are not findings unless they break layout behavior.

Branch rebased onto PR #205 (`pr205-base = fb1fd9234`, flat Card identity). Historical SHAs remain reachable via `wyrd-ui-pre-205`. Previous round's range-diff confirmed preservation; see its subject and verification. Unrelated PR #205/main content is excluded from findings. Do not rerun rebase archaeology without a new concern.

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

## Verification and write constraints

Caller-supplied integrated candidate verification is green: fmt:check, lints, codegen:check, ts:napi:check, ts:test:unit, cargo nextest wyrd-queue; UI package/app check zero errors, tests 40/40, build, real-server Postgres integration file 2/2. Distinguish supplied results, fresh source inspection, and historical browser/human evidence; details in verification.md. Do not run output-generating commands outside this packet.

Only write the assigned report under `changes/active/wyrd-ui-main-integration/review/task-027-39c151769-1/`. No source/test changes, previous packet edits, git changes, commits or push. Reports only; no diff/patch/archive artifacts. HEAD must remain the candidate. A concurrent TASK-003 review packet is separately owned.

## Independent roles

All discovery reviewers receive this same navigation map as a starting point, expand it independently and read the complete cumulative diff themselves. They do not read other current discovery reports or receive an intended verdict. Exact role instructions: `.agents/skills/wyrd-task-review/SKILL.md`.

| Role | Assigned report |
|---|---|
| behavior-rev | task-review-behavior.md |
| invariant-rev | task-review-invariants.md |
| repo-rev | standards-review.md |
| maintainer-rev | maintainer-review.md |
| system-rev | system-review.md |
| browser interaction/reactive lifecycle domain-rev | domain-review-interaction.md |

Follow-up is conditional on claim comparison. Fresh structured ponytail-rev always independently validates discovery, source and prior closure, writing findings-validation.md. Orchestrator writes final verdict only from independently validated findings.
