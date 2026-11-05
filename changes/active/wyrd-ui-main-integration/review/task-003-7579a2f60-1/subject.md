# TASK-003 cumulative re-review subject

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; branch `wyrd-ui-main-integration`.
Immutable candidate: `7579a2f603c023daae0d15b9d5cc3013eec6f00d`.
Cumulative base: `c62ebce8c4de0d43a2b9d50d360de628c49fd533` (parent of `ff4145ac6`, rebased original `695ca914d`). Read `git diff c62ebce8c4de0d43a2b9d50d360de628c49fd533...7579a2f60` independently.
Approved authority: `changes/active/wyrd-ui-main-integration/spec.md` revision 13; original task `tasks/TASK-003-shared-ui-foundation.md` in that packet.
Prior rounds: `review/task-003-ab1d2499f-1/`, `review/task-003-c2bbd8e47-1/`, `review/task-003-a0a5f177f-1/`; FIND-003-1 through FIND-003-10. Read prior verdicts, ledgers and remediation tasks as hypotheses requiring current-source closure.
Current remediation: R3 authored-series identity in prior packet, commit `f0fff4b13`; human-directed flat identity adaptation `review/rebase-flat-identity/TASK-003-R3-flat-card-identity.md`, commits `4eca1e2ba`, `bde4396d9`. Do not retroactively assign that addition a finding ID.

## Navigation map (starting point; expand independently)

- Portable foundation: `crates/wyrd/wyrd-server/wyrd-ui/packages/ui/src/lib/{schemas,contract,loaders,plot,node,mcp,view}.ts`, catalog generation, all components and primitives, public exports/theme.
- Chart producer `plot.ts::timeSeries` -> `Chart.svelte`, sibling bar/table renderers; typed tuple identity, unique display captions, twenty-series folding; query range/duplicate-column selection.
- Card identity: schema -> loadCardHeader dependent reads -> CardHeader/CardRefChip -> BFF/Node/MCP adapters; exact flat kind/name/version/uid, canonical SDK wire shapes and routes.
- Runtime: WyrdView loads/actions/stale cancellation -> remoteLoader -> tenant `/ui/load` BFF -> Rust-owned browser session; Node query stream and MCP host adapter; app `src/app/main.ts`, AppBridge artifact, gallery, examples/ui-app.
- Rust/SDK: shared queue json_result positional projection -> server query/collect and TS native export; shared client browser-session query-only connection; generated declarations; scripts/build.sh production native SDK closure.
- Proof: UI `src/lib/ui-library.test.ts`, `ui-library.integration.test.ts`, fixtures/setup; package scripts, catalog drift checks, focused Rust regression and recorded human visual checkpoint.
- TASK-027 layout code is present in the cumulative range but separately owned. Do not attribute TASK-027 commits or findings to TASK-003, including candidate tip reactive layout reset. Inspect seams only for foundation regression.

## Rebase correspondence and attribution

`git range-diff 695ca914d^..wyrd-ui-pre-205 ff4145ac6^..7579a2f60` maps all reviewed implementation/remediation commits to equal patches, except SDK packaging hunk context (`image` renamed upstream to `bundle_in`) and packet authority-link resolution. Inspect those differences independently. Old range contains upstream commits now integrated in PR #205; they are not TASK-003 findings. Revision 13 only removes space from exact Card reference. PR #205 base `fb1fd9234` is unrelated main content; flat identity adaptation itself is in scope.

## Rules and verification

Read root AGENTS.md, nested applicable rules (TS SDK), `.agents/skills/wyrd-task-review/SKILL.md`, spec-driven development, maintainer guide and applicable architecture/testing/UI authority. No `.codegraph/` directory exists. Compare complete existing workflow owners, not only leaf calls. Never write a diff/source archive into packet.
User supplied candidate-green evidence: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` (package and app zero errors), `test` 40/40, `build`; Postgres-managed `db:migrate:inner` then `WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts` 2/2. Distinguish supplied results from fresh execution; do not infer Linux/image or human/browser reruns.
Source/test files immutable. Only this new review packet is writable. No commit/push. Reviewers own only their assigned report and are not alone in workspace; do not revert others' edits. Do not read peer discovery reports before independent completion. Check HEAD remains candidate. Discovery output requires role-specific coverage and PASS/FAIL/BLOCKED; task implementation roles require full acceptance matrices. Prior findings stay closed only with source evidence.
