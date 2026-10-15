# Immutable review subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/sdk-client-instantiation`
- Base: `2f1db2194f5630df850b10987fe0f360946a525c` (parent of first implementation commit; approved plan present).
- Candidate: `fde7d396a3917964dc344ede2ed09046e179aa66`.
- Full cumulative diff: `git diff 2f1db2194f5630df850b10987fe0f360946a525c fde7d396a3917964dc344ede2ed09046e179aa66`.
- Approved specification: `changes/active/sdk-client-instantiation/spec.md`, revision 9, unchanged across range.
- Original task: `git show 2f1db2194f5630df850b10987fe0f360946a525c:changes/active/sdk-client-instantiation/tasks/TASK-001-unify-sdk-handle-instantiation.md`.
- Current task adds an Implementation Evidence appendix only; the original requirements remain acceptance authority. Appendix lane results are recorded claims, not independent proof or an intended verdict.
- TASK-002 owns authored docsite/README guidance and generated docsite outputs. Review TASK-001's SDK, architecture/reference, executable-example, declaration, and runtime closure, without attributing TASK-002's pending prose to TASK-001.
- No prior task-review verdict or remediation packet exists for this subject.
- Starting tracked working tree is clean. Only review artifacts may be written. Check HEAD remains the candidate before final report; do not implement, format, stage, commit, or change reviewed source.

## Shared navigation map (starting points, not coverage limits)

| Surface | Owners / changed symbols | Callers / proof starting points |
|---|---|---|
| Shared Rust construction and identity | `crates/shared/wyrd-client/src/client.rs` (`connected`, `connect_ambient`); `cards/handle.rs`, `gateway.rs`, `principals/handle.rs`, `operator_connections.rs`, `platform/handle.rs` (`connect`, `connect_with`) | `sdks/wyrd-sdk-rust/src/lib.rs` root/module projections and compile-fail docs; shared `client::tests`; CLI, server main, testing, MCP consumers |
| Bifrost construction and discovery | shared `bifrost/{facade,blocking,query,table,grpc,mod}.rs`; connection option forms, retained client, `list_tables`, table describe names | shared-client integration suites and `wyrd-testing` consumer migrations; existing server list/describe route and compact wire type are reference owners, not new behavior |
| Python lifecycle and public surface | `sdks/wyrd-sdk-python/src/client.rs` (`LazyConnection`), `{bifrost/mod,state/mod,gateway,principals,operators,lib,workflow}.rs`; `python/wyrd/bifrost/_client.py`; public `auth/` rename and authored `.pyi` | `tests/unit/test_sdk_client_instantiation.py`, `tests/integration/test_sdk_client_instantiation.py`, public export tests; other SDK consumers |
| TypeScript boundary | `sdks/wyrd-sdk-ts/native/src/{client,cards,gateway,principals,operators,lib,workflow}.rs`; `wyrd/src/index.ts`; generated `index.d.ts`/`index.d.cts` | unit instantiation/connect-error tests, `tests/integration/sdk-client-instantiation.test.ts`, migrated integration/support consumers |
| Journey and regression closure | Rust `tests/integration/sdk_client_instantiation.rs`, existing support module; Python and TS counterpart journeys | `mise.toml` commands and environment wrappers; `git diff --name-only BASE CANDIDATE` includes all 153 changed paths |
| Authority and guidance | `architecture/wyrd-design.md` Client model; references `architecture/patterns.md`, `languages/{python-api-and-stubs,typescript-guide}.md` | Original spec/task, AGENTS and agent rules, spec-driven development, testing, errors, PyO3 and maintainer references as applicable |

## Available verification claims

Candidate task appendix records format/lint/type lanes (`fmt`, `lints`, Python and TS format/lint/type), unit lanes (`test:shared`, `test:wyrd-sdk`, `py:test:unit`, `ts:test:unit`), generated/boundary lanes (`codegen:check`, `ts:napi:check`, `check:py-wheel-no-testing`, `check:deps`, `check:examples`, diff check), and Rust/Python/TS journey aggregate results. It names focused scenario tests and records verification limits. No separate raw command transcript was supplied. Inspect actual tests, selectors, and asserted outcomes independently. Do not infer acceptance from PASS labels.

Review is static by default. Do not run Cargo/build/generator/format commands or spawn runtime test child processes; request coordinated focused verification from the orchestrator if source evidence cannot resolve a material question. Do not obtain baseline execution or change source. Reviewers may read their own scope and write only their assigned report.

## Required discovery and domain coverage

- behavioral implementation acceptance;
- invariant and state-flow implementation acceptance;
- repository standards across every changed language/layer, including rustdoc and materially modified tests;
- maintainer ownership, reuse, public typing and clarity;
- system failure/recovery, cancellation and retained-resource lifecycle;
- security/identity/domain: selected credential, delegation, platform/tenant separation, error/redaction and discovery authorization;
- concurrency/runtime domain: Python lazy initialization, GIL/runtime bridges, simultaneous first-use, cancellation, async blocking, and retained Bifrost lifecycle.

Read `.agents/skills/wyrd-task-review/SKILL.md` for role-specific report requirements. Use CodeGraph before code lookup when `.codegraph/` exists. All reviewers independently expand this map; findings require reachable source evidence and a concrete approved obligation. No new product decisions, speculative hardening, or optional refactors.
