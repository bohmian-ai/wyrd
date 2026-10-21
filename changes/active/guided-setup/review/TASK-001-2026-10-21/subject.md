# TASK-001 immutable review subject

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/guided-setup`.
Base: `ebd75dc224854413d17cb06b70da8181b78cde6b` (parent of first implementation commit `4188c05f3`).
Candidate: `32fbc264adccaf1160894769d5501fbe441259a5`.
Full range: `git diff ebd75dc224854413d17cb06b70da8181b78cde6b...32fbc264adccaf1160894769d5501fbe441259a5`.
Initial source tree clean; review reports are the only authorized writes. This is the first task review; no prior findings.

Approved authority: `changes/active/guided-setup/spec.md`, revision 5, approved.
Original task and available verification evidence: `changes/active/guided-setup/tasks/TASK-001-guided-setup-and-asgi.md`, including appended evidence. Read evidence as claims to verify, not proof of acceptance. TASK-002 and combined checks explicitly belong to later integration.

## Navigation map

This map starts navigation; independently expand it against source and the complete diff.

- CLI bootstrap: `crates/wyrd/wyrd-cli/src/init.rs` moved from `server/dev.rs`; `cli.rs`, `lib.rs`, server install, gateway, shared client `global_config.rs`; consumers Python/TS installed CLI wrappers and docs commands. CLI `tests/cli.rs::guided_init_print_prompt_is_offline`, `pg_tests::guided_init_lifecycle_and_prompt` plus existing bootstrap regressions.
- Shared prompt owner: `crates/wyrd-spec/src/setup-prompt.md`, `mcp.rs::setup_prompt`; CLI readiness/host launch commands, `mcp/hosts.rs`, server `mcp/guide.rs` and `mod.rs`, CLI `mcp/mod.rs` and `proxy.rs`. Consumers authenticated MCP clients/proxy, Setup docs via mdsvex. Python `test_local_development.py::test_guided_setup_prompt_surfaces`, `test_guided_manual_setup` (three languages).
- Contract correction: `crates/wyrd-spec/src/envelope.rs` Spec JsonSchema, consumed by MCP `cards.get`, OpenAPI and generated clients; trace sibling consumers independently.
- ASGI adapter: `sdks/wyrd-sdk-python/python/wyrd/integrations/asgi/_middleware.py::WyrdMiddleware`, public exports and stub. Existing owners `sdks/wyrd-sdk-python/src/state/mod.rs` lifecycle and `src/observe/mod.rs` Run context, `python/wyrd/otel.py` correlation. Trace startup, app lifespan, final drain, task cancellation, thread context and workers. `tests/integration/test_asgi_integration.py`, `asgi_app.py`, framework telemetry regression tests and state observation tests.
- Docs/examples: Setup tabs, Choose your path, FastAPI guide embedding `examples/integrate-fastapi/app.py`; docs generated llms files, command checker, mdsvex highlight, landing loader `docs/src/routes/+page.server.ts` and shell-links fixture. `mise.toml` wires journey lanes, pyproject/uv.lock worker dependencies.
- Governing files: root AGENTS; nested docs, Python SDK, TS SDK AGENTS; `changes/spec-driven-development.md`; `agent_docs/index.md` then applicable rust/errors/agent-surfaces/maintainer-style guides; `TESTING.md`; `docs/CONTRIBUTING.md` and PRODUCT as applicable. Architecture map `architecture/README.md`: Wyrd design Client model, runtime/observation identity and public contracts; security posture local credential persistence and privacy; operations deployment local bundles and supported topology; Bifrost System boundary lifecycle.

## Review discipline

Follow `.agents/skills/wyrd-task-review/SKILL.md` for your assigned role/report. Use CodeGraph before locating/reading code (`.codegraph` exists): `mise exec -- codegraph explore "<symbols or question>"`. Read every touched nested AGENTS. Every reviewer reads complete actual cumulative diff. Do not modify candidate source, run formatters, commit, or write diff/source archives. Independent reviewers must not read other discovery reports. Reports include exact source evidence, complete scope coverage, verification limits and PASS/FAIL/BLOCKED. Do not treat style preferences as blockers; missing changed Rust item rustdoc remains blocking. No speculative improvements or extra features. Audit workflow reuse, not just leaf calls. Run commands through mise; avoid concurrent Cargo builds. Existing runtime evidence is Linux x86_64 only, with macOS arm64/Linux arm64 unexercised. Any probes must be read-only to source. Notify orchestrator of candidate changes or missing authority.
