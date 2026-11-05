# Immutable cumulative TASK-003 re-review

Repository: /Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration; branch wyrd-ui-main-integration.
Base: c62ebce8c4de0d43a2b9d50d360de628c49fd533 (ff4145ac6 parent).
Candidate: f8c6f083103d8f55007510a4fbcc75e36b4d9c55.
Authority: changes/active/wyrd-ui-main-integration/spec.md revision 13; original tasks/TASK-003-shared-ui-foundation.md; root/applicable nested AGENTS.md; changes/spec-driven-development.md; agent_docs/maintainer-style.md and applicable architecture/testing guides.

Each reviewer must independently read git diff BASE...CANDIDATE and expand this starting navigation map. No .codegraph/ exists. Prior packets: task-003-ab1d2499f-1, task-003-c2bbd8e47-1, task-003-a0a5f177f-1, task-003-7579a2f60-1, task-003-39c151769-1, task-003-6aace6e07-1, task-003-21dab5d11-1. Prior findings FIND-003-1..15 must receive source-backed closure. Latest remediation: preceding packet TASK-003-R7-range-contract-and-sql-binds.md; commits bca10dcfa and f8c6f0831; use 21dab5d11...candidate to locate fixes, retain full cumulative acceptance.

## Navigation map

- UI packages/ui: schemas.ts (bounds/RelativeRangeSchema/resolveRange), loaders.ts (loadPanel/loadChart/chartParams/highestBind), contract/catalog/generator, plot, Node/MCP adapters, renderer, theme, Chart/CardHeader/primitives and approved six exports.
- Call chains: validated view/panel range -> shared resolution -> dependent loader -> local/MCP or remote HTTP executor; typed series and positional columns -> plots/captions; exact flat Card reads -> headers/chips/action references.
- Runtime: WyrdView supersession/actions -> remote loader -> session-bound POST /t/[tenantKey]/ui/load -> Rust-owned browser session; portable Node/MCP reads, stream settlement/cancellation, bounded result admission.
- Rust/SDK: shared wyrd-queue json_result.rs -> server query/collect.rs and SDK native NativeBifrostQueryStream::collect; budget parse_ceiling and stream lifecycle; shared-client query-only browser-session transport; TS exports/declarations.
- Build/hosts/proof: scripts/build.sh, package exports, AppBridge artifact, gallery, examples/ui-app; UI ui-library.test.ts and ui-library.integration.test.ts; Rust/SDK focused tests, catalog/import/build proof and historical human checkpoints.

PR #205/main (pr205-base fb1fd9234, flat Card identity) is not a finding. Earlier reviews confirmed preserved code after rebase. TASK-027 WyrdView layout passed at 39c151769 and is not TASK-003 scope; trace shared foundation seams without re-owning layout.

## Verification and write boundaries

Supplied green at candidate: UI pnpm check; unit tests 40/40; catalog --check; mise run fmt:check; UI real-server integration file 2/2. No Rust changed in R7. Historical broader results and human checkpoints remain historical. Do not run builds/tests that write outside this packet; static source/Git reads and artifact-free probes only. No new browser, platform or image qualification is implied.

Write ONLY the assigned report in this directory; no source/test edits, diffs/patches/archives, staging, commits or pushes. You are not alone: do not revert others' edits. Discovery agents must not read peer reports before finishing. Apply full .agents/skills/wyrd-task-review/SKILL.md role requirements. Check immutable HEAD.
