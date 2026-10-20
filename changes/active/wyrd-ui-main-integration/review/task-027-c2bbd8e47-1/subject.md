# TASK-027 cumulative remediation re-review

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
Original base `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`; immutable candidate `c2bbd8e47e4752b3772ad75acfaf23f3400ac704` (clean HEAD at intake).
Read full `git diff a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb...c2bbd8e47` for cumulative acceptance; latest `git diff ab1d2499f...c2bbd8e47` locates remediation owners but does not substitute for full review.
Approved spec: `changes/active/wyrd-ui-main-integration/spec.md`, revision 12.
Original task: `changes/active/wyrd-ui-main-integration/tasks/TASK-027-view-layout.md`.
Prior verdict/validated FIND ledger: `changes/active/wyrd-ui-main-integration/review/task-027-ab1d2499f-1/verdict.md`, `changes/active/wyrd-ui-main-integration/review/task-027-ab1d2499f-1/findings-validation.md`.
Remediation task plus appended execution evidence: `changes/active/wyrd-ui-main-integration/review/task-027-ab1d2499f-1/TASK-027-R1-layout-state-and-geometry.md`.
Read prior findings as hypotheses and independently verify closure at candidate; preserve prior FIND IDs. New IDs start after FIND-003-7 or FIND-027-3, respectively. Do not use peer reports from this attempt during discovery.

## Starting map (expand independently)

UI root `crates/wyrd/wyrd-server/wyrd-ui`; portable owner `packages/ui/src/lib`.
TASK-003: loaders.ts chartParams/loadChart/shapeChart/CardHeader dependent reads; plot.ts columnIndex/timeSeries/barRows; components Chart, CardHeader, ActionButton and new ActionNote; contract.ts wyrdProblem and generated problems.json via scripts/catalog.ts; node.ts stream cancellation, mcp.ts errors, view.ts remoteLoader/mount; WyrdView lifecycle/actions; BFF src/routes/t/[tenantKey]/ui/load/+server.ts and examples/ui-app/src/routes/view/load/+server.ts; package/browser-native split; build.sh packing and shared Rust client query-only seam remain cumulative scope.
TASK-027: layout.ts rowsFor/place/edit and geometry constants; WyrdView retained/draft state, commit/key/drag cleanup, intrinsic bind:offsetHeight/placements/boxStyle/mobile; view.ts mount callback; app/main.ts per-render MCP host, gallery and examples/ui-app hosts. Existing components supply intrinsic content, not placement authority.
Tests: src/lib/ui-library.test.ts now ten cases, plus cumulative ui-library.integration.test.ts and original Rust named tests. Original-task human visual acceptance 2026-10-20 remains supplied evidence; new remediation browser evidence is appended to remediation tasks. Applicable sensitive domain: UI interaction/accessibility and geometry.

## Scope attribution and authority

TASK-003 original scope starts 0ccdeef86 through a1e9b48d2, includes 7acf4b55e/e8b0c42f2, excludes unrelated 45797e42e/2884da447/62966bbdd/fc0d28fda and merged upstream changes. Its prerequisites 9e8e14998/0bc456a07/695ca914d are in original base; inspect owner integration without attributing pre-existing defects. Original TASK-027 implementation d33de26e4/d90d98a12 is separately authorized successor, not TASK-003 drift. Latest TASK-003 remediation commits 966c32b53/6c73a0f3f/238a0310d/0bb0422f8/003629cb3/cbc8edd60; TASK-027 remediation b512c2221/84d5dd22e. Each task gets its own review; inspect shared seams and attribute a finding to its owning task once.

Read AGENTS.md, architecture/agent-rules.md, reference router, spec-driven-development.md, maintainer-style.md, approved spec/original task, applicable architecture/testing/UI authorities and role requirements in `.agents/skills/wyrd-task-review/SKILL.md`. No .codegraph directory exists. Reuse existing navigation tools. Audit existing complete owners/alternatives for new machinery. No source edits or new review harness/scripts; only your assigned report may be written; others work concurrently.

Evidence is appended to remediation task, not proof by assertion: nine exact focused commands, ten-test file, UI/example check/build, codegen and diff checks reported green. Independently assess assertions and recorded diagnosis. Linux has no execution evidence; live degraded-terminal parity remains historical unverified limit. Signed-geometry initial RED was produced in prior review rather than separately rerun by implementer; assess against repository TDD rules and substantive proof without inventing churn. Example app is a workspace consumer; canonical checks use UI workspace --filter @wyrd/example-ui-app. Direct cwd resolution failure is a supplied observation to trace against actual supported usage, not an intended finding or verdict.

Required discovery reports: task-review-behavior.md, task-review-invariants.md, standards-review.md, maintainer-review.md, system-review.md, and assigned domain report. Every report must cover complete applicable scope, prior-finding closure with source evidence, material new/remaining findings and overall PASS/FAIL/BLOCKED. Implementation roles need full acceptance matrix. Followup and fresh independent validation follow discovery. Missing reports or changed candidate block review. Do not implement, merge, push, deploy or edit prior records.
