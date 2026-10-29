# TASK-003 immutable review subject

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
Candidate: `ab1d2499f6a08a6b5afe1f6c904e72746d526bda` (HEAD, clean at intake).
Base: `695ca914df61a8683059f6f32279ec2148424e06` (`0ccdeef86^`).
Read the complete `git diff 695ca914d...ab1d2499f` and history; task implementation ends at `a1e9b48d2` but review resulting source at the candidate.

Authority: approved `changes/active/wyrd-ui-main-integration/spec.md` revision 12; original `tasks/TASK-003-shared-ui-foundation.md` in that packet. Its evidence section records commands/results, not independent proof. Human visual acceptance explicitly approved 2026-10-20. TASK-027 is a separately approved successor, reviewed separately; do not classify its authorized layout changes as TASK-003 drift. Precursor dependencies `9e8e14998`, `0bc456a07`, `695ca914d` are already in the base; inspect their owners/callers for integration correctness, without attributing pre-existing defects to this task.

User explicitly excludes unrelated commits `45797e42e`, `2884da447`, `62966bbdd`, `fc0d28fda`. Inspect their diffs to distinguish them; do not include their independent changes as task findings. `7acf4b55e` and `e8b0c42f2` ARE in scope (bundle and shared client). Evidence-only later commits do not extend production scope. No `.codegraph/` exists at intake; use normal repository navigation.

## Starting navigation map (expand independently)

- UI owner `crates/wyrd/wyrd-server/wyrd-ui/packages/ui`: package.json six exports; schemas.ts/contract.ts; loaders.ts dependent reads, validation and shaping; node.ts and mcp.ts executor boundaries; view.ts validation/mount/remoteLoader; WyrdView.svelte lifecycle/action dispatch; components Chart/CardHeader and eight primitives; catalog generation scripts/catalog.ts; AppBridge src/app/main.ts and vite.app.config.ts.
- Consumers: UI `src/routes/t/[tenantKey]/ui/load/+server.ts`, `src/lib/server/auth/browser-sessions.ts`; `src/routes/_gallery`, `src/lib/ui-fixtures.ts`; `examples/ui-app` direct and view routes, executor and server endpoint.
- Rust owner: `crates/shared/wyrd-client/src/bifrost/facade.rs` connect_with/read_only/from_sink; `src/client.rs` browser-session discrimination and regression test. Follow existing session transport and Bifrost consumers.
- Packaging/codegen: scripts/build.sh bundle, mise.toml ts:build/codegen:check, UI workspace/manifests/lock, brand/gen-theme.mjs.
- Tests: UI src/lib/ui-library.test.ts (TASK-003 scenarios 1/3); ui-library.integration.test.ts (scenarios 2/4, real SDK/server/MCP/AppBridge); shared client exact test and server query/MCP tests named in original task. Existing query normalization owner `wyrd_queue::json_result::JsonResultBuilder`.

## Review process and limits

Read AGENTS.md, architecture/agent-rules.md, reference router, spec-driven-development.md, maintainer-style.md and applicable design/security/Bifrost/deployment/testing/UI authorities. Apply the exact role instructions in `.agents/skills/wyrd-task-review/SKILL.md`. Source immutable; write only the assigned report. Other reviewers are writing their reports concurrently; do not modify or read their reports during discovery. Independently inspect diff and source, including existing alternatives to new orchestration. No implementation edits, merges, pushes or deployment. Verification recorded in task includes green UI tests/check/build, custom app checks/build, codegen, Rust focused tests and live scenarios; limits: no live degraded-terminal parity and local bundle closure only darwin-arm64. Do not silently treat missing platform proof as green. Report exact source evidence, consequences, smallest testable correction and overall PASS/FAIL/BLOCKED. Do not invent optional hardening.
